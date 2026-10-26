# ropacker

Arma un lugar de Roblox (`.rbxlx`) desde carpetas de Luau y lo maneja con Open Cloud: publicar, reiniciar servidores y correr Luau en un servidor real. Es un solo binario de Rust, sin dependencias en tiempo de ejecución.

## Instalar

```sh
cargo install --path .     # deja el comando `ropacker` en ~/.cargo/bin
# o sin instalar:  cargo run -- build
```

## Configurar (`ropacker.toml` en la raíz del proyecto)

```toml
universe = 1234567890
place = 9876543210
base = "place/base.rbxlx"      # el lugar exportado de Studio (File > Save to File As > .rbxlx)
output = "build/place.rbxlx"
env = "secrets/.env"           # archivo con ROBLOX_API_KEY (también lee .env junto al toml)

[test]                         # opcional: otra experiencia (p. ej. [DEV]); `run` corre ahí, y `publish/restart --test`
universe = 1234567891
place = 9876543211

[tree]                         # ruta en el juego = carpeta cuyo contenido va ahí
ReplicatedStorage = "game/shared"
ServerScriptService = "game/server"
"StarterPlayer.StarterPlayerScripts" = "game/client"
```

En cada ruta de `[tree]` se cambian los hijos que tienen el mismo nombre que algo de la carpeta. Lo demás del lugar base se queda como está: Workspace, ajustes de los servicios y lo que se hizo a mano en Studio. Si cambias algo de eso en Studio, vuelve a exportar el lugar base.

Archivos: `X.luau` → ModuleScript, `X.server.luau` → Script, `X.client.luau` / `X.local.luau` → LocalScript, carpeta → Folder (o el script de su `init.luau`, con lo demás adentro). También `.lua`. No se incluyen los ocultos ni los que no son Luau.

## Comandos

| Comando | Qué hace | Permiso de la clave |
|---|---|---|
| `ropacker build` | Arma el `.rbxlx` | — |
| `ropacker publish [--restart] [--saved]` | Lo sube como versión publicada (o solo guardada) | `universe-places:write` (+ `universe:write` con `--restart`) |
| `ropacker restart [--all-places]` | Reinicia los servidores para que todos pasen a la última versión | `universe:write` |
| `ropacker run script.luau` | Sube una versión **guardada, sin publicar**, y corre el script en un servidor de Roblox con ese código. Muestra los logs y lo que devuelve; sale con 1 si falla | `universe-places:write`, `universe.place.luau-execution-session:read` y `:write` |
| `ropacker run script.luau --version N` / `--live` | Lo mismo sobre una versión que ya existe / la publicada | los de Luau |

Con `[test]`, `run` sube y corre en esa experiencia (con `--prod`, en la de verdad): el permiso de Luau da acceso a los DataStores, así que conviene darlo solo a la experiencia de pruebas. `publish --test` y `restart --test` también van a ella. Si existe `ROBLOX_TEST_API_KEY`, se usa esa clave para la experiencia de `[test]`.

`publish` no publica sin lugar base, porque borraría todo lo que no viene de las carpetas. Para hacerlo igual, usa `--allow-no-base`.

Límites de Roblox: 30 publicaciones por minuto; 5 tareas de Luau por minuto, de hasta 5 minutos cada una, con 450 KB de logs.

## Pruebas

```sh
cargo test
```

También se puede usar como librería (`ropacker::{tree, rbxlx, cloud, config}`): los errores vuelven como `anyhow::Result`, no cierran el proceso.
