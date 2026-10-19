fn main() {
    // Der Kernel wird mit unserem eigenen Linker-Skript gelinkt,
    // damit er im oberen Adressbereich liegt und Limine die Requests findet.
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/linker.ld");
    println!("cargo:rerun-if-changed=linker.ld");
}
