import ast,subprocess,time,sys
from pathlib import Path
from pathlib import Path
command=['python3',str(Path(__file__).resolve().with_name('build-if-idle.py')),*sys.argv[1:]]
deadline=time.monotonic()+900
while True:
    child=subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    first=child.stdout.readline();print(first,end='',flush=True)
    tail=child.stdout.read();status=child.wait()
    if tail:print(tail,end='',flush=True)
    if status!=75:raise SystemExit(status)
    prefix='NOT_STARTED; Cargo observed: '
    if not first.startswith(prefix):raise SystemExit(75)
    known=ast.literal_eval(first[len(prefix):].strip())
    while True:
        live=[]
        for pid,start,_ in known:
            try:
                fields=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
                if fields[19]==start and fields[0]!='Z':live.append(pid)
            except FileNotFoundError:pass
        if not live:break
        if time.monotonic()>=deadline:
            print('Admission deferred; same known Cargo identities remain live:',live,flush=True);raise SystemExit(75)
        time.sleep(.2)
