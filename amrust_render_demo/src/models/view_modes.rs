#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShadingMode {
    Shade,
    #[default]
    ShadeAndWire,
    WireOnly,
}

impl ShadingMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            ShadingMode::Shade => "Shade",
            ShadingMode::ShadeAndWire => "Shade and Wire",
            ShadingMode::WireOnly => "Wire Only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LightingMode {
    Lit,
    #[default]
    Unlit,
}

impl LightingMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            LightingMode::Lit => "Lit",
            LightingMode::Unlit => "Unlit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MaterialOpacity {
    #[default]
    Opaque,
    Transparent,
}

impl MaterialOpacity {
    pub fn display_name(&self) -> &'static str {
        match self {
            MaterialOpacity::Opaque => "Opaque",
            MaterialOpacity::Transparent => "Transparent",
        }
    }
}
