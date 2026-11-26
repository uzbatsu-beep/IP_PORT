# Third-party font notices

IP_PORT uses egui 0.37.0 and its epaint_default_fonts 0.37.0 dependency with the default fonts feature. The application license remains MIT; the bundled font files retain their upstream terms.

Upstream: [egui 0.37.0 font directory](https://github.com/emilk/egui/tree/841a328ba2dd8f6996beb7189ebb7fc00e668a2a/crates/epaint_default_fonts/fonts). Upstream commit: `841a328ba2dd8f6996beb7189ebb7fc00e668a2a`.

The original upstream notice files are included in [third-party/epaint_default_fonts](third-party/epaint_default_fonts/):

- [Hack-Regular.txt](third-party/epaint_default_fonts/Hack-Regular.txt) — Hack, DejaVu and Bitstream Vera notices.
- [OFL.txt](third-party/epaint_default_fonts/OFL.txt) — SIL Open Font License 1.1 (Noto Emoji upstream optional font).
- [UFL.txt](third-party/epaint_default_fonts/UFL.txt) — Ubuntu Font Licence 1.0 (Ubuntu Light, Canonical).
- [egui-icons.txt](third-party/epaint_default_fonts/egui-icons.txt) and [emoji-icon-font-mit-license.txt](third-party/epaint_default_fonts/emoji-icon-font-mit-license.txt) — upstream icons and John Slegers' MIT notice.
- [LICENSE-MIT](third-party/epaint_default_fonts/LICENSE-MIT) and [LICENSE-APACHE](third-party/epaint_default_fonts/LICENSE-APACHE) — egui upstream code licenses.

The complete upstream font notice set is retained even where a font is optional. No font binary has been copied into this repository. Review these notices and actual enabled features when upgrading egui; the Cargo Deny exception is scoped to the exact audited package version. This file addresses the newly identified font obligations; it is not a complete audit of every third-party license obligation in the product.
