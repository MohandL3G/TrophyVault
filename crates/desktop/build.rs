fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/trophyvault-icon.ico");
        let _ = res.compile();
    }
}
