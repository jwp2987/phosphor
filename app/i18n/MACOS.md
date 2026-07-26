# What needs to be done to add a new locale on macOS

For macOS to translate correctly, when adding a new locale (e.g. `zh-TW`, `ja`, etc.), you also need to:

1. **Update the macOS plist configuration**: add the new locale code to the
   `CFBundleLocalizations` array in every plist that needs to declare
   localization:
   - `app/assets/resources/mac/CLI-Info.plist` — edit the `<array>` under `<key>CFBundleLocalizations</key>`
   - `app/src/bin/local.rs` — edit the plist XML inside the
     `embed_plist::embed_info_plist_bytes!` macro
   - `app/src/bin/oss.rs` — edit the plist XML inside the
     `embed_plist::embed_info_plist_bytes!` macro
   - Example: add `<string>zh-TW</string>` to the `<array>`
2. **Update the build script**: add the new locale code to the
   `plutil -insert/replace CFBundleLocalizations` command in the
   `script/update_plist` script.
