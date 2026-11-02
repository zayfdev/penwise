# Third-party notices

Penwise includes code ported from the following projects. Their license terms apply to the ported portions.

## perfect-freehand

Ported to Rust in `crates/penwise-core/src/outline/freehand.rs` and `crates/penwise-core/src/outline/walker.rs`
(modified: time-based smoothing replaces streamline, 1 px start gate, distance-window start pressure, fixed-length
tapers, libm trigonometry).

Source: https://github.com/steveruiz/perfect-freehand

```
MIT License

Copyright (c) 2021 Stephen Ruiz Ltd

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Geist and Geist Mono

Bundled as the Android chrome fonts in `android/app/src/main/res/font/geist.ttf` and `geist_mono.ttf` (unmodified
variable fonts from https://github.com/google/fonts, `ofl/geist` and `ofl/geistmono`; upstream
https://github.com/vercel/geist-font). Copyright 2024 The Geist Project Authors, licensed under the SIL Open Font
License 1.1. The licence texts ship in the app at `android/app/src/main/assets/licenses/geist-OFL.txt` and
`geist-mono-OFL.txt`.
