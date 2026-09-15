fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("packaging/windows/viewlume.ico");
        res.compile().expect("failed to compile windows resources");
    }
}
