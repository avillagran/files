# Files frente a Flea 0.4.0 — NODE, 2026-11-03

El usuario autorizó levantar la congelación de recursos/arranque exclusivamente
para decidir si Files merece continuar por ser más liviano **y** más rápido que
Flea 0.4. No se amplió funcionalidad, instaló ninguna app ni cambió asociaciones.

**Resultado: ventaja clara de RAM para Files; velocidad de listado mixta. La
condición estricta de ganar en ambas dimensiones no se cumple en esta prueba.**
No equivale a que Flea siempre gane o a que Files deba borrarse. No continuar
ampliando funcionalidades por inercia: esta evidencia informa la decisión del usuario.

## Resultados

Medianas de cuatro pasadas válidas por app/tamaño (16 total), alternando AB/BA.
Memoria estable del árbol de procesos: suma PSS+SwapPss, incluye helpers observados.
La memoria GPU se registra aparte y no se suma ciegamente a PSS.

| Archivos | App | RAM estable MiB | Pico muestreado MiB | Estado listado listo ms | Ventana registrada ms |
|---|---|---:|---:|---:|---:|
| 10.000 | Files | 18,9 | 18,9 | 97 | 97 |
| 10.000 | Flea 0.4 | 107,9 | 108,6 | 403 | 399 |
| 100.000 | Files | 32,6 | 32,6 | 603 | 99 |
| 100.000 | Flea 0.4 | 110,5 | 111,3 | 409 | 405 |

Files reduce aproximadamente82,5%/70,5% la RAM del árbol en estas dos carpetas.
Con100k, el estado del listado de Files tarda aproximadamente47,4% más que Flea.
Ventana registrada no prueba que ya pueda usarse. Ninguna columna mide input
nativo→frame presentado ni permite afirmar superioridad global de respuesta.

## Protocolo y trazabilidad

- NODE x86_64, AMD Ryzen9 5900X; Wayland real Hyprland0.56.2, pantalla1920x1080,
  escala1. Clientes655x836 en idéntica posición/geometría en todas las pasadas;
  cada aplicación conserva su densidad de filas y estilo por defecto.
- Flea tagv0.4.0/22fa922, binario oficial linux-x86_64. SHA256 del tar verificado
  contra archivo publicado: efa466c4916228cad6e374f9f33b216ef236f53515758fddc502c6972f5c50d7.
  Cada log confirma Vulkan NVIDIA GeForce RTX3080Ti, no fallback software.
- Files release compilado en NODE desde5a8f995, Cargo normal/original idle gate,
  build terminado antes de medir. Software FLTK/fltk-rs1.5.23; sin instalar.
  SHA de los cuatro ejecutables en assessment.json.
- Carpetas propias btrfs con10k/100k archivos.txt vacíos, mismos nombres, List y
  orden por nombre. Perfiles privados HOME/XDG; no preferencias/cachés habituales.
  Metadatos del directorio precalentados antes de cada tamaño; procesos nuevos.
  **No son pruebas de caché fría real**: no se vació caché global del equipo.
- Muestreo árbol cada≈40–59ms, cinco segundos por pasada y al menos tres tras
  readiness. Mediana estable desde max(ready+1s,3s). Picos muestreados son cotas
  inferiores; hijos que nazcan/mueran entre muestras pueden escapar. Cero errores
  del sampler en16 pasadas válidas. No se usó el contador PSS del footer de Files.
- Readiness Files: FILES_METRICS_READY→scene_ready,loaded,errorfree,count exacto.
  Flea: IPC listingWindowState total exacto/loaded>0,state ready,drawnCount>0 del
  adaptador de vista montada. Son observadores diferentes; latencia externa por
  polling (decenas de ms), con overhead IPC de Flea incluido. Files no espera stat
  de todas las filas en List. Capturas posteriores verifican listado visible y
  conteo100k en ambos, fuera de las pasadas cronometradas.
- GPU: nvidia-smi registra memoria G/C+G de Flea en dos GPUs; no se interpreta
  como total independiente sumable a PSS ni se usa para ganar la comparación.
- Escritorio cotidiano con otras apps: no se cerraron ni mataron procesos ajenos.
  Alternancia reduce sesgo de orden; no es un laboratorio de máquina ociosa.
- Un fallo inicial del parser de readiness en Flea100k: IPC aún no devolvía JSON;
  app no falló, harness sí. Log íntegro conservado; se corrigió a not-yet-ready y
  repitió como2-retry. Cuatro válidas por grupo; fallida excluida explícitamente.
- Solo grupos de procesos creados por el ensayo se terminaron conSIGTERM. Esto
  no es prueba de cierre normal, recuperación ni ausencia de fugas a largo plazo.

## Lo que falta para una conclusión más amplia

No se midieron arranque realmente frío, búsqueda/navegación/scroll mediante input
nativo, latencia de presentación, miniaturas equivalentes, consumo prolongado,
CPU/energía ni otras GPUs/máquinas. No había driver de computer-use disponible;
no se improvisó inyección de teclado ni se hizo pasar un handler por inputSO.
Las apps tampoco tienen paridad total de funciones: este ensayo cubre List con
archivos vacíos. La ventaja de RAM sí existe en este escenario; la superioridad
universal de velocidad no está demostrada y el caso100k favorece a Flea.

[Evidencia, hashes, archivos raw y límites](evidence/flea-0.4-comparison-20261103/assessment.json).
[Gráfico](evidence/flea-0.4-comparison-20261103/comparison.html).
Fuentes oficiales: [release](https://github.com/thisisgm/flea/releases/tag/v0.4.0),
[método](https://github.com/thisisgm/flea/blob/v0.4.0/docs/benchmarks.md).
El README del tag todavía muestra métricas0.3.10: no se usaron como resultados0.4.
La autorización excepcional no levanta la congelación para otros experimentos.

## Subsequent optimization

The original results above remain historical evidence. Sourceff7af56 subsequently
improved worker/UI wake-ups: NODE100k readiness206ms vsFlea408ms, with retained
RAM advantage. M2 was separately evaluated and still has open memory/native
acceptance. See [LISTING-EVENT-WAKE.md](LISTING-EVENT-WAKE.md); this does not claim
universal superiority or erase the earlier mixed result.
