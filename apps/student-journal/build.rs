fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon/oxidium.ico");
        res.set("ProductName", "Oxidium — Student Journal");
        res.set(
            "FileDescription",
            "Oxidium Local-First Student Journal Desktop Application",
        );
        let _ = res.compile();
    }
}
