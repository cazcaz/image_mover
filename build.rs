fn main() {
    // Only embed resources on Windows
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("ProductName", "Image Mover");
        res.set("FileDescription", "Image Mover Application");
        res.set("OriginalFilename", "image_mover.exe");
        res.set("FileVersion", "0.1.0.0");
        res.compile().unwrap();
    }
}
