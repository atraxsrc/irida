fn main() {
    println!("cargo:rerun-if-changed=targets");
    println!("cargo:rerun-if-changed=rice");
}
