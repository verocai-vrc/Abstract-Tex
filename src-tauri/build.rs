// Runs before the crate compiles. `tauri_build` reads tauri.conf.json, embeds the icon and
// manifest on Windows, and copies `externalBin` sidecars (Tectonic) next to the built executable.
fn main() {
    tauri_build::build()
}
