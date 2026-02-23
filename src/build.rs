fn main() {
    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .csharp_class_name("IrisNative")
        .csharp_namespace("MOGA_Vision.Native")
        .generate_csharp_file("../MOGA-Vision/NativeMethods.g.cs") // 确保路径指向你的 WPF 项目
        .unwrap();
}
