pub(crate) trait ServerAddress: Sync + Send + 'static {
    fn host(&self) -> String;

    fn port(&self) -> u32;

    fn is_available(&self) -> bool;
}
