# JPEG progresivo grande: preview DC1/8 — 0.0.4 (2026-10-27)

Unidad funcional parcial verificada en Omarchy. El JPEG baseline12/24MP ya usaba IDCT reducido, pero muchos
progresivos válidos se rechazaban porque sus coeficientes completos excedían32MiB.
El fallback nuevo conserva ese límite y usa una muestra DC por bloque:1/8 por eje.
No aumenta32MiB ni150MB; no acredita consumo real del árbol mientras el freeze
siga activo. Objetivo50MB y baseline/startupFAIL históricos permanecen abiertos.

El decoder vendored MIT/Apache se mantiene; no se adoptó libjpeg/IJG como
dependencia de files o helper. Sin paquete nuevo, FFI nativo de producto, cache
persistente ni archivo temporal de preview. El compressed input se presta al
Cursor; se mantienen8MiB input,40Mipíxeles/8192 por eje,3MiB RGB,64scans/1MiB
metadata y workers inmediatos. RLIMIT_AS96MiB no equivale a PSS. Timeout/cancelación
se supervisan mediante kill/reap existentes, sin bloquear UI. Se reutilizan la
firma FD esperada/efectiva, RootProof de incrustadas y caché/renderer existentes.

Sólo cuando el progresivo excedería32MiB de coeficientes completos se activa el
modoDC; baseline y progresivo pequeño conservan la ruta anterior. DCinitial y
refinements se parsean/validan; ACentropy se omite hasta siguiente marcador,
por lo que NO es verificación de integridad AC ni reconstrucción a resolución
original. Malformed headers/DC, duplicados, escala incorrecta y componentes DC
ausentes fallan; una corrupción exclusivamente AC puede no impedir este preview.
El bitmap1/8 y tamaño original son visibles en el footer; zoom no recupera detalle
que no está en ese bitmap. Alta calidad/chroma completo y otras variantes siguen
pendientes, no se cierra P02 entero con esta unidad.

La ruta nueva coincide byte por byte con el decoder completo a1/8 en un JPEG
progresivo de color generado por ImageMagick, incluidos DCrefinements. Una
comparación inicial con djpeg4:2:0 falló (hasta20/255 en bordes del gradiente):
no era el mismo muestreo. ProbeC nativo con API del libjpeg-turbo3.2.0 instalado
confirma Y IDCT1×1 pero Cb/Cr2×2, mientras jpeg-decoder usa1×1 en todos. El decoder
completo anterior tiene el mismo desvío; no se atribuye al almacenamientoDC ni se
relaja ese umbral para esconderlo. Con4:4:4 y escalas equivalentes, referencia
externa pasa≤3/255. Fixtures originales sintéticos pequeños y comandos en
`crates/helper/tests/fixtures/README.md`; no foto ajena ni nuevo requisito nativo
de build/test de Cargo. ScriptC referencia opcional, no programa de producto.

Procedencia: `.crate` SHA coincide SOURCE.json,17fuentes originales verificadas,
patchfuzz0 reconstruye exactamente src actuales, MIT/Apache/README intactos.
`evidence/jpeg-dc/vendor-source-review.json`; referencia de escalas y FAILinicial
preservados. Doctests upstream del vendor buscan imágenes no incluidas y fallaron;
10unitvendor --lib pasan, no false PASS de aquel comando.

## Verificación e instalación

445Rust141core/50helper/254UI PASS, vendor10--lib aparte,
Clippyworkspacealltargets-Dwarnings y release limpios. Logs
`target/jpeg-dc-workspace-test.log`, `jpeg-dc-helper-test.log`,
`jpeg-dc-vendor-lib-test.log`, `jpeg-dc-clippy.log`, `jpeg-dc-release.log`.
El parche anterior sí aplicaba al archive pero no reproducía8fuentes ya formateadas
en53998a5; rustfmt del resultado anterior reproduce exactamente esas8. La nueva
reconstrucción incorpora ese drift preexistente de formato, además del algoritmo
nuevo sólo en decoder.rs; no cambia licencias ni inventa auditoría anterior vigente.

`evidence/0.0.4/jpeg-large-ui-1793078251643976150.json` funcionalPASS:
baseline12/24MP y EXIF siguen visibles; progresivo12MP→500×375,
24MP→750×500 y EXIF6→500×750 reales,33colores distintos como mínimo y firma FD
efectiva. Ready fresco después de cada comando,6capturas. No fuente gris de
reemplazo ni sólo un menú/core. Report+sidecar explican escala compositor2 con
ventana física2000×1300 pero framebuffer1000×650: no nitidez nativa2x probada.
Handlers internos/framebuffer, NOinputSO. Cierre con intención pendiente,
previewdrain y exit0,8identidadesPID/start helpers ausentes/reutilizadas,
Z rechazado; helper vivo justo al cerrar NO_VALIDATED, no lifetimes completos.

Una expectativa antigua de rechazo en el verificador se cambió a Ready fresco+
dimensiones/RGB; los informes antiguos se preservan. El checker de captura ahora
comprueba helpers registrados, no un conjunto vacío de sampler; versión previa
sólo acreditaba raíz, tal como sus reportes declaraban. Fixturegeneration/convert
usa TMPDIR/MAGICK_TEMPORARY_PATH privado en cache, sin /tmp ni config global.
No mide PSS/Swap/CPU/arranque. Recursos/startupNOT_MEASUREDfreeze.


488b450 instalado con backup/hash coherentes, informe
`evidence/0.0.4/installed-checkpoint-1793078897751186933.json`:
files9748f0c0996c77a5b1e7f9f41aa5aafb20c868786d595618a6aa5c267137809e;
helper3872dd9dae0bab4eb06f659dc33b595f45d4029134dd85a7d24e4298f7f55d5a;
HTMLa5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
