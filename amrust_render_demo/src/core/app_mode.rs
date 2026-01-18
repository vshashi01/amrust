/// High level modes of the application
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppMode {
    /// Objects mode represents the Objects as it is (Unique Parts only)
    Objects,

    /// Build mode represent the Objects as they will be printed with printed positions
    Build,
}
