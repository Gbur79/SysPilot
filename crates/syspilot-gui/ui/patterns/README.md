# Slint 1.18 Pattern Catalog (SysPilot Architecture)

Verified UI architectural patterns and component templates for the SysPilot Slint GUI (`crates/syspilot-gui`).

## Slint 1.18 Invariants

1. **Explicit Layout Sizing (`vertical-stretch: 0`):** Fixed chrome, headers, action buttons, alert banners, and telemetry rows must explicitly specify `vertical-stretch: 0` to prevent unintended flex-expansion.
2. **Zero Absolute Positioning:** Never use absolute `x` or `y` coordinates. Rely entirely on `HorizontalLayout` and `VerticalLayout` with layout alignment and padding.
3. **Ellipsis Text Safeguards:** All dynamic telemetry labels, descriptive messages, and path strings must have `overflow: elide` to prevent layout clipping or text collisions.
4. **Theme Design Tokens:** All colors and radiuses must reference design tokens from `Theme` (`../theme.slint`), ensuring cohesive design language across skins.
5. **Dual-Skin Parity (`is_lcars_skin`):** Every pattern must support both LCARS Okuda (`is_lcars_skin: true`) and Classic Cyberpunk (`is_lcars_skin: false`) modes with identical data contracts.
6. **Typed Callbacks:** Actions and event handlers must use typed callbacks with payload identifiers (e.g. `callback action_triggered(string);`) guarded against re-entrancy while `busy`.

## Verified Component Templates

- `AlertBannerPattern` (`alert_banner_pattern.slint`): Expandable system alert / attention banner with badge pill and elided message track.
- `ActionCallbackPattern` (`action_callback_pattern.slint`): Single-action trigger with normalized 175px width, busy state handling, and re-entrant guard.
- `TelemetrySectionPattern` (`telemetry_section_pattern.slint`): Pod header and telemetry row combination adapting seamlessly between LCARS and Classic card layouts.
- `PatternsShowcase` (`patterns_showcase.slint`): Hermetic validation window compiling all 3 pattern components.
