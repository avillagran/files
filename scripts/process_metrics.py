#!/usr/bin/env python3
"""Muestreo Linux de un árbol observado; no incluye memoria GPU ni picos entre muestras."""
import errno
import os
import time
from pathlib import Path


def read_stat(text):
    # comm puede contener espacios y paréntesis; los campos numéricos van después.
    pid = int(text.split(' ', 1)[0])
    fields = text[text.rindex(')') + 2:].split()
    return {'pid': pid, 'ppid': int(fields[1]), 'state': fields[0],
            'ticks': int(fields[11]) + int(fields[12]),
            'threads': int(fields[17]), 'start': int(fields[19])}


class TreeSampler:
    """Instanciar mientras root está vivo; sample() devuelve datos o errores explícitos.

    Conserva descendientes ya observados incluso si son reparentados. Procesos que
    nacen y mueren entre muestras pueden escapar: el pico es un límite inferior.
    discovery='children' evita el escaneo global usando todos los TID/children;
    no es un snapshot atómico, puede perder procesos entre lecturas y depende de
    acceso a task/stat/children. No cambia el modo predeterminado 'scan'.
    CPU es porcentaje de un núcleo durante el intervalo observado, puede superar 100.
    """
    def __init__(self, pid, proc=Path('/proc'), clock=time.monotonic, *, discovery='scan'):
        if discovery not in ('scan', 'children'):
            raise ValueError("discovery must be 'scan' or 'children'")
        self.discovery = discovery
        self.proc = Path(proc)
        self.clock = clock
        root = self._stat(pid)
        self.root = (pid, root['start'])
        self.known = {self.root}
        self.previous = {}
        self._unresolved_children = {}
        self.last_time = None
        self.hz = os.sysconf('SC_CLK_TCK')

    def _stat(self, pid):
        return read_stat((self.proc / str(pid) / 'stat').read_text())

    def _confirmed_exit(self, pid, start):
        try:
            after = self._stat(pid)
        except FileNotFoundError:
            try:
                (self.proc / str(pid)).stat()
            except FileNotFoundError:
                return 'process directory disappeared'
            except OSError:
                pass
            return None
        except (OSError, ValueError, IndexError):
            return None
        if after['start'] != start:
            return 'identity changed (PID reused)'
        if after['state'] == 'Z':
            return 'same identity is a zombie (address space exited)'
        return None

    @staticmethod
    def _missing(exc):
        return (isinstance(exc, FileNotFoundError) or
                isinstance(exc, OSError) and exc.errno in (errno.ENOENT, errno.ESRCH))

    def _discovery_table(self):
        """No /proc-wide enumeration: inspect every task of each retained member.

        A child mentioned by the kernel is relevant even before its first readable
        stat. Never discard a live/unverifiable child just because it is unknown.
        """
        table, errors, exits, events = {}, [], [], []
        pending, inspected = [], set()

        def error(pid, phase, exc, **context):
            errors.append({'pid': pid, 'phase': phase, 'error': str(exc), **context})

        def ended(pid, start, phase, reason, **context):
            exits.append({'pid': pid, 'start': start, 'phase': phase,
                          'reason': reason, 'memory_available': False, **context})

        def absent(path):
            try:
                path.stat()
            except FileNotFoundError:
                return True
            except OSError:
                pass
            return False

        def load(pid, expected=None, parent=None):
            if pid in table:
                self._unresolved_children.pop(pid, None)
                return
            if expected is None and parent is not None:
                self._unresolved_children.setdefault(pid, (parent, table[parent]['start']))
            try:
                stat = self._stat(pid)
                if stat['pid'] != pid:
                    raise ValueError('stat PID does not match requested identity')
            except (OSError, ValueError, IndexError) as exc:
                reason = None
                if self._missing(exc):
                    if expected is not None:
                        reason = self._confirmed_exit(pid, expected)
                    elif absent(self.proc / str(pid)):
                        reason = 'newly discovered process directory disappeared before identity read'
                if reason:
                    self._unresolved_children.pop(pid, None)
                    ended(pid, expected, 'discovery_stat', reason, parent_pid=parent)
                else:
                    error(pid, 'discovery_stat', exc, parent_pid=parent, start=expected)
                return
            if expected is not None and stat['start'] != expected:
                ended(pid, expected, 'discovery_stat', 'identity changed (PID reused)')
                return
            # A reused PID may now be unrelated. A live relationship mismatch is
            # unverifiable discovery, not permission to count an unrelated process.
            if parent is not None and stat['ppid'] != parent and (pid, stat['start']) not in self.known:
                error(pid, 'discovery_relationship', 'listed child no longer has the observed parent',
                      start=stat['start'], parent_pid=parent, observed_ppid=stat['ppid'])
                return
            table[pid] = stat
            self._unresolved_children.pop(pid, None)
            self.known.add((pid, stat['start']))
            if stat['state'] != 'Z':
                pending.append(pid)

        for pid, start in sorted(self.known):
            load(pid, expected=start)
        # Unknown identities cannot silently fall out of scope after reparenting.
        # Retry only under the same verified parent identity; otherwise retain an
        # explicit unresolved discovery until the PID directory disappears (or a
        # fresh, verified children list establishes a current relationship).
        for pid, (parent, parent_start) in sorted(self._unresolved_children.items()):
            if pid in table:
                self._unresolved_children.pop(pid, None)
            elif parent in table and table[parent]['start'] == parent_start:
                pass  # Retry only after this owner's task-list/identity validation.
            elif absent(self.proc / str(pid)):
                self._unresolved_children.pop(pid, None)
                ended(pid, None, 'discovery_stat', 'unresolved process directory disappeared', parent_pid=parent)
            else:
                error(pid, 'discovery_unresolved', 'previously listed child still exists without a verifiable identity/parent',
                      parent_pid=parent, parent_start=parent_start)
        while pending:
            pid = pending.pop()
            owner = table[pid]
            identity = (pid, owner['start'])
            if identity in inspected:
                continue
            inspected.add(identity)
            taskdir = self.proc / str(pid) / 'task'
            try:
                tasks = list(taskdir.iterdir())
            except OSError as exc:
                reason = self._confirmed_exit(pid, owner['start']) if self._missing(exc) else None
                if reason:
                    ended(pid, owner['start'], 'discovery_tasks', reason)
                else:
                    error(pid, 'discovery_tasks', exc, start=owner['start'])
                continue
            if not tasks:
                error(pid, 'discovery_tasks', 'empty task enumeration for a surviving process', start=owner['start'])
            discovered_children = {child for child, parent_identity in self._unresolved_children.items()
                                   if parent_identity == identity}
            for task in tasks:
                if not task.name.isdigit():
                    error(pid, 'discovery_tasks', 'non-numeric task directory')
                    continue
                tid, before, phase = int(task.name), None, 'task_stat'
                try:
                    before = read_stat((task / 'stat').read_text())
                    if before['pid'] != tid:
                        raise ValueError('task stat PID does not match TID')
                    phase = 'task_children'
                    child_text = (task / 'children').read_text()
                    children = child_text.split()
                    if any(not child.isascii() or not child.isdigit() or int(child) <= 0 for child in children):
                        raise ValueError('malformed task children list')
                    discovered_children.update(int(child) for child in children)
                    phase = 'task_identity'
                    after = read_stat((task / 'stat').read_text())
                    if after['pid'] != tid:
                        raise ValueError('task stat PID does not match TID')
                    if after['start'] != before['start']:
                        events.append({'pid': pid, 'start': owner['start'], 'tid': tid,
                                       'task_start': before['start'], 'phase': phase,
                                       'reason': 'task identity changed (TID reused)', 'memory_available': False})
                except (OSError, ValueError, IndexError) as exc:
                    # Vanished tasks are a discovery lifetime gap. Missing files
                    # inside a still existing task, parse and permission errors
                    # invalidate; they must not become an empty children list.
                    reason = None
                    if self._missing(exc):
                        if absent(task):
                            reason = 'task directory disappeared during discovery'
                        else:
                            reason = self._confirmed_exit(pid, owner['start'])
                    if reason:
                        events.append({'pid': pid, 'start': owner['start'], 'tid': tid,
                                       'task_start': before['start'] if before else None,
                                       'phase': phase, 'reason': reason, 'memory_available': False})
                    else:
                        error(pid, phase, exc, start=owner['start'], tid=tid)
            # Do not trust a task list read across reuse of the containing PID.
            owner_verified = False
            try:
                after = self._stat(pid)
                if after['pid'] != pid:
                    raise ValueError('stat PID does not match requested identity')
                if after['start'] != owner['start'] or after['state'] == 'Z':
                    ended(pid, owner['start'], 'discovery_identity',
                          'identity changed (PID reused)' if after['start'] != owner['start'] else 'same identity is a zombie (address space exited)')
                else:
                    owner_verified = True
            except (OSError, ValueError, IndexError) as exc:
                reason = self._confirmed_exit(pid, owner['start']) if self._missing(exc) else None
                if reason:
                    ended(pid, owner['start'], 'discovery_identity', reason)
                else:
                    error(pid, 'discovery_identity', exc, start=owner['start'])
            if owner_verified:
                for child in sorted(discovered_children):
                    load(child, parent=pid)
            elif discovered_children:
                error(pid, 'discovery_children_identity', 'children list crossed an unverifiable parent identity; listed children not counted',
                      start=owner['start'], child_pids=sorted(discovered_children))
        return table, errors, exits, events

    def sample(self):
        now = self.clock()
        exits, discovery_events = [], []
        if self.discovery == 'children':
            table, errors, exits, discovery_events = self._discovery_table()
            selected = set(table)
        else:
            table, errors = {}, []
            known_pids = {pid for pid, _ in self.known}
            for entry in self.proc.iterdir():
                if entry.name.isdigit():
                    try:
                        stat = self._stat(int(entry.name))
                        table[stat['pid']] = stat
                    except (OSError, ValueError, IndexError) as exc:
                        # Un PID conocido no legible no demuestra que haya terminado.
                        # Sólo omitir ENOENT cuando también desapareció su directorio.
                        if int(entry.name) in known_pids:
                            gone = False
                            if isinstance(exc, FileNotFoundError):
                                try:
                                    entry.stat()
                                except FileNotFoundError:
                                    gone = True
                                except OSError:
                                    pass
                            if not gone:
                                errors.append({'pid': int(entry.name),
                                               'error': f'stat no verificable: {exc}'})
            selected = {pid for pid, s in table.items() if (pid, s['start']) in self.known}
            while True:
                children = {pid for pid, s in table.items() if s['ppid'] in selected}
                extra = children - selected
                if not extra:
                    break
                selected |= extra
        totals = {'pss_kib': 0, 'swap_pss_kib': 0, 'threads': 0, 'fds': 0}
        processes, current = [], {}
        root_survived = True
        delta_ticks = 0
        for pid in sorted(selected):
            s = table[pid]
            identity = (pid, s['start'])
            self.known.add(identity)
            if s['state'] == 'Z':
                continue
            phase = "smaps_rollup"
            try:
                base = self.proc / str(pid)
                values = {}
                for line in (base / 'smaps_rollup').read_text().splitlines():
                    key, _, rest = line.partition(':')
                    if key in ('Pss', 'SwapPss'):
                        values[key] = int(rest.split()[0])
                if set(values) != {'Pss', 'SwapPss'}:
                    raise ValueError('smaps_rollup incompleto')
                phase = "fd"
                fds = sum(1 for _ in (base / 'fd').iterdir())
                phase = "identity"
                after = self._stat(pid)
                if after['start'] != s['start']:
                    exits.append({'pid': pid, 'start': s['start'], 'phase': phase, 'reason': 'identity changed (PID reused)', 'memory_available': False})
                    continue
                if identity == self.root:
                    root_survived = after['state'] != 'Z'
                row = {'pid': pid, 'start': s['start'], 'pss_kib': values['Pss'],
                       'swap_pss_kib': values['SwapPss'], 'threads': after['threads'], 'fds': fds}
                processes.append(row)
                for key in totals:
                    totals[key] += row[key]
                current[identity] = after['ticks']
                if identity in self.previous:
                    delta_ticks += max(0, after['ticks'] - self.previous[identity])
            except (OSError, ValueError, IndexError) as exc:
                # A missing metric is never a zero-valued live process. Only an
                # ENOENT followed by verified exit/reuse can omit an ended identity;
                # report that lifetime gap explicitly. Permission/parse failures
                # continue to invalidate the sample, including unverifiable stat.
                ended = self._confirmed_exit(pid, s['start']) if isinstance(exc, FileNotFoundError) or isinstance(exc, OSError) and exc.errno in (errno.ENOENT, errno.ESRCH) else None
                if ended:
                    exits.append({'pid': pid, 'start': s['start'], 'phase': phase, 'reason': ended, 'memory_available': False})
                else:
                    errors.append({'pid': pid, 'error': str(exc)})
        elapsed = now - self.last_time if self.last_time is not None else None
        cpu = delta_ticks / self.hz / elapsed * 100 if elapsed and elapsed > 0 else None
        self.last_time, self.previous = now, current
        root_stat = table.get(self.root[0])
        root_alive = bool(root_stat and root_stat['start'] == self.root[1] and root_stat['state'] != 'Z' and root_survived and not any((e['pid'], e['start']) == self.root for e in exits))
        return {**totals, 'memory_kib': totals['pss_kib'] + totals['swap_pss_kib'],
                'time_monotonic': now, 'cpu_percent_observed': cpu,
                'root_alive': root_alive, 'process_count': len(processes),
                'complete': not errors and root_alive, 'errors': errors, 'processes': processes,
                'exited_during_read': exits, 'discovery_events': discovery_events,
                'discovery': self.discovery,
                'completeness_scope': 'complete verifies surviving readable identities, not full lifetimes; confirmed exits with unread memory are explicit; sampled peaks are lower bounds'}
