# Archive execution evidence

Source-only core phase, not an installed extraction feature.

- `final-workspace-tests.log`:1088 PASS (387 core/91 helper/610 UI),3 existing ignored.
- `final-clippy.log`:workspace/all-targets with -Dwarnings PASS.
- `final-workspace-release.log`:final optimized workspace build.
- `final-release.log`:optimized explicit core example build (not app installation).
- `final-fmt.log`:fmt check PASS.
- `final-native-execution.json`:12 optimized child-process contracts PASS, independent
  Python zipfile source fixtures; `native-execution-check.py` reproduces them with
  an explicit new --report path (existing evidence is never overwritten).
- `installed-unchanged.json`:five installed and five backup hashes match the prior
  SFTP-report checkpoint; target/release is NOT installed.
- `checks.json`:scope/source hashes and pending work.

Earlier compile E0509 (move from Drop fixture) and warnings in first tests remain
in original logs; later tests borrow the fixture path. Original native report is
retained; final run followed an allocation-capacity guard/API change. Neither is
GUI, queue, native input, kernel-D, failed-fsync syscall or powerloss proof.20
filesystem unit contracts include synthetic fault hooks; flags distinguish observed
publication/cleanup and explicit retry authority. No resource/startup sampling.

The global objective stays active; historical global failures and preview pause
remain unchanged. Next is bounded queue/AuxIo/review UI, not another codec engine.
