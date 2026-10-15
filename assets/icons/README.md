# Bundled icons

The two SVGs in `icons/scalable/actions/` are Font Awesome Free icons:

| File | Font Awesome icon | Source |
| --- | --- | --- |
| `fa-display-symbolic.svg` | `fa-solid fa-display` | <https://fontawesome.com/icons/display> |
| `fa-window-maximize-symbolic.svg` | `fa-regular fa-window-maximize` | <https://fontawesome.com/icons/window-maximize> |

Font Awesome Free is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)
for the icons and SIL OFL 1.1 for the font. The required attribution is kept in
the comment at the top of each SVG.

They are compiled into the binary (see `build.rs`) and registered as a
`hicolor` icon theme; see `src/icons.rs` for the details.
