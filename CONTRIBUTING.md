# Contributing to codwawzpc / Contribuir a codwawzpc

[English](#english) · [Español](#español)

## English

`codwawzpc` is an experimental Linux PC port of Call of Duty: World at War: Zombies, based on touchHLE. This guide covers contributions to this fork. Reports and changes are reviewed through this repository's [issues](https://github.com/aurelioochoa/codwawzpc/issues) and [pull requests](https://github.com/aurelioochoa/codwawzpc/pulls).

### Scope

The current target is **Linux x86_64** and the **iPhone v1.5.0** release, bundle `com.activision.callofduty`. Contributions to the host, importer, controls, launcher, loading, saves, documentation and tests are useful.

General compatibility with other iPhone applications is outside the current scope. Read the [README](README.md), [launcher guide](launcher/README.md) and [observed results and limitations](docs/smoke-test/README.md) before describing a problem.

### Reports and proposals

Open an [issue](https://github.com/aurelioochoa/codwawzpc/issues/new/choose) in English or Spanish and include:

- Your `codwawzpc` revision (`git rev-parse HEAD`), Linux distribution and architecture.
- GPU/driver and desktop session when the problem concerns rendering, window behavior or input.
- Reproduction steps, expected and observed results, and whether an earlier revision worked.
- Game version, the result of `make check-game` and relevant logs. Do not attach the IPA or extracted game files; remove private details from logs.

For larger proposals, explain the problem and intended design to help review. Distinguish build/component checks from real gameplay observations; reaching the menu does not establish later-round stability.

### Development setup

To submit changes, fork this repository on GitHub, clone your fork and create a branch from `main`. Follow the [README requirements and setup](README.md#requirements), retaining `Cargo.lock` and pinned submodules.

```sh
git submodule update --init --recursive
make setup
make build
```

The launcher needs Python 3.10+, Tk and Pillow; FFmpeg plays the original intro. Synthetic checks need no game files. Real-game checks require your own compatible, unencrypted IPA and a graphical desktop.

### Changes and verification

Keep each pull request focused on one problem and add tests when they catch a meaningful regression. Preserve imported originals and saves; use synthetic fixtures or isolated installations for tests. Follow the [existing coding style](dev-docs/code-style.md).

| Command | Purpose |
| --- | --- |
| `make fmt-check` | Check Rust formatting |
| `make check` | Check the library and binaries without linking |
| `make test` | Run library tests without the upstream iOS test toolchain |
| `make test-cli` | Exercise the CLI/importer with synthetic fixtures |
| `make verify` | Check, build, run formatting, host/relay tests and synthetic CLI tests |
| `make test-launcher` | Run isolated launcher checks; requires a graphical display |
| `make check-game` | Verify your imported game installation |

Run `make verify` for runtime, importer or build-system changes. Add `make test-launcher` for launcher changes. For documentation, check links, examples and formatting; a runtime rebuild is unnecessary. State what you checked and identify any checks you could not run.

### Pull requests

Target **`aurelioochoa/codwawzpc:main`**. Describe the problem, resulting behavior, verification and any remaining limitations or regressions. Include screenshots when they help review visual changes. The maintainer reviews changes on GitHub; Gerrit and touchHLE review identifiers are not part of this fork's workflow.

### AI assistance

AI-assisted contributions are allowed in this fork. Contributors remain responsible for understanding and reviewing the content, verifying its behavior, and checking its provenance and license compatibility. Describe substantial AI assistance in the pull request.

This policy applies to `codwawzpc`. Changes submitted to upstream touchHLE must follow upstream's own contribution rules.

### Provenance and licensing

Contribute only material you have the right to share. Prefer public documentation, reproducible observations and original work; do not include copied proprietary code, leaked material, IPAs or extracted game assets. Follow the [piracy policy](CODE_OF_CONDUCT.md#english).

Preserve license and copyright headers, dependency notices and attribution history. Changes to MPL-covered code remain subject to its terms. New host code currently follows MPL-2.0; components retain their own licenses and the documented combined binary is GPL-3.0-or-later. See [LICENSE](LICENSE), [project notices](NOTICE.md) and [upstream terms](README.touchHLE.md#license). Changing these contribution instructions does not change the software licenses.

## Español

`codwawzpc` es un port experimental de Call of Duty: World at War: Zombies para Linux, basado en touchHLE. Esta guía describe las contribuciones a este fork. Los informes y cambios se revisan en los [issues](https://github.com/aurelioochoa/codwawzpc/issues) y [pull requests](https://github.com/aurelioochoa/codwawzpc/pulls) de este repositorio.

### Alcance

El objetivo actual es **Linux x86_64** y la versión **de iPhone v1.5.0**, con identificador `com.activision.callofduty`. Son útiles las mejoras del host, importador, controles, lanzador, cargas, partidas, documentación y pruebas.

La compatibilidad con otras aplicaciones de iPhone queda fuera del alcance actual. Consulta el [README](README.md), la [guía del lanzador](launcher/README.md) y los [resultados y limitaciones observados](docs/smoke-test/README.md) antes de describir un problema. Las dos últimas referencias están en inglés.

### Informes y propuestas

Abre un [issue](https://github.com/aurelioochoa/codwawzpc/issues/new/choose) en inglés o español e incluye:

- La revisión de `codwawzpc` (`git rev-parse HEAD`), distribución Linux y arquitectura.
- GPU/controlador y sesión gráfica cuando el problema afecte al renderizado, ventana o entrada.
- Pasos para reproducirlo, resultado esperado y observado, y si funcionaba en una revisión anterior.
- Versión del juego, resultado de `make check-game` y registros pertinentes. No adjuntes el IPA ni archivos extraídos del juego; elimina datos privados de los registros.

Para propuestas amplias, explica el problema y el diseño previsto para facilitar la revisión. Distingue las pruebas de compilación o componentes de las observaciones reales del juego; llegar al menú no demuestra compatibilidad con toda la partida.

### Preparar el entorno

Para enviar cambios, crea un fork en GitHub, clónalo y abre una rama a partir de `main`. Sigue los requisitos y la preparación del [README](README.md#requisitos), conservando `Cargo.lock` y los submódulos fijados.

```sh
git submodule update --init --recursive
make setup
make build
```

El lanzador requiere Python 3.10 o posterior, Tk y Pillow; FFmpeg reproduce la introducción original. No necesitas el juego para las comprobaciones sintéticas. Las pruebas reales requieren tu propio IPA compatible sin cifrar y un escritorio gráfico.

### Cambios y pruebas

Mantén cada pull request centrado en un problema y añade pruebas cuando permitan detectar una regresión significativa. Conserva los archivos originales importados y las partidas; usa datos sintéticos o instalaciones aisladas para las pruebas. Respeta el [estilo existente](dev-docs/code-style.md).

| Comando | Función |
| --- | --- |
| `make fmt-check` | Comprobar el formato de Rust |
| `make check` | Comprobar la biblioteca y binarios sin enlazar |
| `make test` | Ejecutar pruebas de biblioteca sin la cadena de herramientas iOS original |
| `make test-cli` | Comprobar CLI e importador con datos sintéticos |
| `make verify` | Comprobar, compilar y ejecutar formato, pruebas de biblioteca y CLI sintética |
| `make test-launcher` | Probar el lanzador de forma aislada; requiere pantalla gráfica |
| `make check-game` | Verificar tu instalación importada |

Ejecuta `make verify` para cambios del runtime, importador o sistema de compilación. Añade `make test-launcher` para cambios en el lanzador. Para documentación, comprueba enlaces, ejemplos y formato; no hace falta recompilar el runtime. Indica qué comprobaste y cualquier comprobación que no pudiste ejecutar.

### Pull requests

Envía el pull request a **`aurelioochoa/codwawzpc:main`**. Describe el problema, el comportamiento resultante, las pruebas y cualquier límite o regresión pendiente. Incluye capturas si ayudan a revisar cambios visuales. El mantenedor revisa los cambios en GitHub; Gerrit y los identificadores de revisión de touchHLE no forman parte del proceso de este fork.

### Uso de IA

Este fork permite contribuciones asistidas por IA. Quien contribuye sigue siendo responsable de comprender y revisar el contenido, comprobar su funcionamiento y verificar su procedencia y compatibilidad de licencia. Describe en el pull request el uso sustancial de IA.

Esta política se aplica a `codwawzpc`. Si envías cambios al proyecto touchHLE original, debes seguir sus propias reglas de contribución.

### Procedencia y licencias

Aporta únicamente material que puedas compartir. Prioriza documentación pública, observaciones reproducibles y trabajo original; no incluyas código propietario copiado, material filtrado, IPA ni recursos extraídos del juego. Cumple la [política de piratería](CODE_OF_CONDUCT.md#español).

Conserva las cabeceras de licencia y derechos de autor, los avisos de dependencias y el historial de atribución. Los cambios en código cubierto por MPL continúan bajo sus términos. El código nuevo del host sigue actualmente MPL-2.0; los componentes conservan sus propias licencias y el binario combinado documentado es GPL-3.0-or-later. Consulta [LICENSE](LICENSE), los [avisos del proyecto](NOTICE.md) y las [condiciones originales](README.touchHLE.md#license). Cambiar estas instrucciones no cambia las licencias del software.
