# Evaluación posterior del resultado

El segundo FAIL es del harness: buscó fixture_note="preserve" sin espacios, aunque config.toml conserva el valor mediante serialización TOML con espacios. Los tres procesos aún no formaban parte de este alcance: esta ejecución tuvo dos salidas normales y las capturas previas. Se conserva el FAIL original.
