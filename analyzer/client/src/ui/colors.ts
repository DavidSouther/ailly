/**
 * Self-contained stand-in for `@nominal-io/colors` (BadgeColor only).
 * Values are plain-ish class strings; since Tailwind isn't configured in
 * this project, these are mostly used as opaque tokens by ui/badges/badge.
 */

export enum BadgeColor {
  SKY = "badge-sky",
  MINT = "badge-mint",
  AMBER = "badge-amber",
  BERRY = "badge-berry",
  PLUM = "badge-plum",
  METAL = "badge-metal",
  METAL_DARK = "badge-metal-dark",
}
