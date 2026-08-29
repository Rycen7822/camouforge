//! Windows 图标必须以资源 ID 1 嵌入；GPUI 与托盘代码都按该 ID 加载。

fn main() {
    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=resources/app.ico");

    #[cfg(target_os = "windows")]
    {
        embed_resource::compile("resources/app.rc", embed_resource::NONE);
    }
}
