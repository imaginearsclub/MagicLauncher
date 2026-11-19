fn main() {
    // No VC++ runtime install needed on the owner's PC (same as pack-publisher.exe).
    static_vcruntime::metabuild();
    tauri_build::build()
}
