fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon/oxidium.ico");
        res.set("ProductName", "Oxidium Student Journal MCP Server");
        res.set(
            "FileDescription",
            "Oxidium Local Model Context Protocol Server",
        );
        let _ = res.compile();
    }
}
