// Copyright © 2026 Kirky.X. All rights reserved.

//! Marker types for modules without configuration or dependencies.

/// Marker type for modules that do not require configuration.
/// Modules using `NoConfig` do not need to call `.config()` on their builder.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConfig;

/// Marker type for modules that do not require dependencies.
/// Modules using `NoRequirements` do not need to call `.requirements()` on their builder.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRequirements;
