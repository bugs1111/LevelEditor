# Third-party notices

The standalone Godot plugin under `importer/godot/addons/ldtk-importer/` integrates [heygleeson/godot-ldtk-importer](https://github.com/heygleeson/godot-ldtk-importer), commit `0ecab8d1cc6531801b487157848059c3b8971aa0`. Copyright (c) 2023 Andrew Gleeson, MIT. The complete license accompanies the plugin in `importer/godot/addons/ldtk-importer/LICENSE`; provenance and local changes are recorded in `importer/godot/UPSTREAM.md`. This independent GDScript plugin is not a Rust editor runtime dependency.

This project ports LDtk by Sébastien Bénard / deepnight. Reference source commit: `2b7b5512f2f88fa281cb6492debbb26ead7838d4`, https://github.com/deepnight/ldtk . The upstream MIT notice is retained in `LICENSE`. The empty project fixture/template and schema-derived definition defaults originate from that project.

The bundled `examples/topdown/Typical_TopDown_example.ldtk` and `atlas/TopDown_by_deepnight.png` originate from the upstream official samples. Their source sample notices are retained in `examples/topdown/UPSTREAM_SAMPLES_README.md`. The complete original sample library is also retained under `examples/original/`, including its original README and individual asset notices. This includes Cavernas (Adam Saltsman), SunnyLand (Ansimuz), Inca (Kronbits), and Nuclear Blaze (Sébastien Bénard, CC BY-SA 4.0). See `examples/original/README.md` for attribution links.

Roboto Condensed fonts originate from LDtk's bundled font assets. Their Apache license is retained in `assets/fonts/LICENSE.txt`.

`assets/fonts/LevelEditorCJK-Regular.otf` is a renamed subset of Noto Sans CJK SC 2.004 by Adobe. It includes GB 2312 and the Simplified Chinese interface glyphs. The SIL Open Font License 1.1 and original copyright are retained in `assets/fonts/NotoSansCJK-OFL.txt`.

Icons under `assets/icons/` originate from LDtk's bundled UI assets; upstream credits Material icons in its README. The Rust port does not claim authorship of those resources.

Rust dependency licenses remain those of their respective crates, recorded by `Cargo.lock` and each dependency's package metadata. No upstream application runtime is shipped.

Winit 0.30.13 is vendored with the [X11 wheel-release fix](https://github.com/rust-windowing/winit/commit/03dad26c43f91cf4c985fb0a7dafa8f3ef48b746) backported to its 0.30 event handler. Winit's Apache 2.0 license is included in `vendor/winit/LICENSE` and in packaged assets as `Winit-LICENSE.txt`.

Original LDtk bitmap font atlases and metrics (Noto Sans Display and Pixel Berry), built-in tile atlas, home illustrations and application icon are retained under `assets/`. They remain attributed to their upstream authors and are used as original port resources. The bundled Noto Sans Display source font identifies copyright 2015 Google Inc. in its font metadata; its SIL Open Font License 1.1 is retained in `assets/bitmap_fonts/NotoSansDisplay-OFL.txt` (license text: https://github.com/google/fonts/blob/main/ofl/notosansdisplay/OFL.txt).

Resource provenance is not a blanket MIT relicensing of independent art. The pinned upstream stores `pixel_berry.png` / `.xml` with the family name "Pixel Berry 08/84 Ltd.Edition", but no separate font license was found there. The embedded `finalbossblues-icons_full_16.png` comes from the LDtk 1.5.4 preview commit `53cfd539` and is credited by upstream `Lang.getEmbedAtlasInfos` to FinalBossBlues, https://finalbossblues.itch.io/icons . The pinned source contains no separate redistribution grant for that atlas. Keep these original credits and establish any additional permissions required before public redistribution; the port does not claim ownership or a new license for these resources.

The original changelog text and release illustrations under `assets/CHANGELOG.md` and `assets/changelogImg/` also originate from the same pinned LDtk source. They are displayed with a Rust Markdown viewer.

The native syntax definitions `assets/syntax/Haxe.sublime-syntax` and `assets/syntax/TTCNConfig.sublime-syntax` translate the CodeMirror 5 Haxe and TTCN configuration tokenizers bundled in LDtk 1.5.3 (`app.asar/node_modules/codemirror/mode/haxe/haxe.js` and `mode/ttcn-cfg/ttcn-cfg.js`). Source project: https://codemirror.net/5/ . They run through Rust syntect; no CodeMirror JavaScript runtime is shipped. The upstream license follows:

```text
MIT License

Copyright (C) 2017 by Marijn Haverbeke <marijn@haverbeke.berlin> and others

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
