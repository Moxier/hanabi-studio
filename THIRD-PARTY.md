# Third-party software in Hanabi Studio

Hanabi Studio is MIT-licensed. It is built with the open-source Rust libraries below; each keeps its own licence, and all of them allow use in an MIT program. When several licences are listed with OR, Hanabi Studio uses the first permissive one (for example `self_cell` under Apache-2.0). Full licence texts are in each library's source package (`cargo vendor` or crates.io).

The built-in interface fonts come from `epaint_default_fonts` (Ubuntu Font Licence 1.0 and SIL Open Font Licence 1.1). Japanese and Chinese text uses fonts already installed on the computer; none are shipped.

Not included: **ffmpeg**, used for video export, is a separate program the person installs (LGPL/GPL). Hanabi Studio only starts it.

| Library | Version | Licence |
|---|---|---|
| accesskit | v0.25.2 | MIT OR Apache-2.0 |
| adler2 | v2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| ahash | v0.8.12 | MIT OR Apache-2.0 |
| allocator-api2 | v0.2.21 | MIT OR Apache-2.0 |
| android-activity | v0.6.1 | MIT OR Apache-2.0 |
| android-properties | v0.2.2 | MIT |
| approx | v0.5.1 | Apache-2.0 |
| arrayvec | v0.7.8 | MIT OR Apache-2.0 |
| as-raw-xcb-connection | v1.0.1 | MIT OR Apache-2.0 |
| atomic-waker | v1.1.2 | Apache-2.0 OR MIT |
| bitflags | v1.3.2 | MIT/Apache-2.0 |
| bitflags | v2.13.2 | MIT OR Apache-2.0 |
| bit-vec | v0.8.0 | Apache-2.0 OR MIT |
| block2 | v0.5.1 | MIT |
| block2 | v0.6.2 | MIT |
| bumpalo | v3.20.3 | MIT OR Apache-2.0 |
| bytemuck_derive | v1.12.1 | Zlib OR Apache-2.0 OR MIT |
| bytemuck | v1.25.2 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | v0.1.0 | Unlicense OR MIT |
| byteorder | v1.5.0 | Unlicense OR MIT |
| bytes | v1.12.1 | MIT |
| calloop | v0.13.0 | MIT |
| calloop-wayland-source | v0.3.0 | MIT |
| cfg-if | v1.0.5 | MIT OR Apache-2.0 |
| cgl | v0.3.2 | MIT / Apache-2.0 |
| color_quant | v1.1.0 | MIT |
| color | v0.3.3 | Apache-2.0 OR MIT |
| combine | v4.6.9 | MIT |
| concurrent-queue | v2.5.0 | Apache-2.0 OR MIT |
| core_detect | v1.0.0 | MIT/Apache-2.0 |
| core-foundation-sys | v0.8.7 | MIT OR Apache-2.0 |
| core-foundation | v0.9.4 | MIT OR Apache-2.0 |
| core-graphics-types | v0.1.3 | MIT OR Apache-2.0 |
| core-graphics | v0.23.2 | MIT OR Apache-2.0 |
| crc32fast | v1.5.4 | MIT OR Apache-2.0 |
| crossbeam-deque | v0.8.8 | MIT OR Apache-2.0 |
| crossbeam-epoch | v0.9.21 | MIT OR Apache-2.0 |
| crossbeam-utils | v0.8.23 | MIT OR Apache-2.0 |
| cursor-icon | v1.2.0 | MIT OR Apache-2.0 OR Zlib |
| dispatch2 | v0.3.1 | Zlib OR Apache-2.0 OR MIT |
| dispatch | v0.2.0 | MIT |
| dlib | v0.5.3 | MIT |
| document-features | v0.2.12 | MIT OR Apache-2.0 |
| downcast-rs | v1.2.1 | MIT/Apache-2.0 |
| downcast-rs | v2.0.2 | MIT OR Apache-2.0 |
| dpi | v0.1.2 | Apache-2.0 AND MIT |
| ecolor | v0.37.0 | MIT OR Apache-2.0 |
| eframe | v0.37.0 | MIT OR Apache-2.0 |
| egui_glow | v0.37.0 | MIT OR Apache-2.0 |
| egui | v0.37.0 | MIT OR Apache-2.0 |
| egui-winit | v0.37.0 | MIT OR Apache-2.0 |
| either | v1.19.0 | MIT OR Apache-2.0 |
| emath | v0.37.0 | MIT OR Apache-2.0 |
| ena | v0.14.4 | MIT OR Apache-2.0 |
| encoding_rs | v0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| epaint_default_fonts | v0.37.0 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| epaint | v0.37.0 | MIT OR Apache-2.0 |
| equivalent | v1.0.3 | Apache-2.0 OR MIT |
| errno | v0.3.14 | MIT OR Apache-2.0 |
| euclid | v0.22.14 | MIT OR Apache-2.0 |
| fdeflate | v0.3.7 | MIT OR Apache-2.0 |
| fearless_simd | v0.4.1 | Apache-2.0 OR MIT |
| flate2 | v1.1.11 | MIT OR Apache-2.0 |
| foldhash | v0.2.0 | Zlib |
| font-types | v0.12.7 | MIT OR Apache-2.0 |
| foreign-types-macros | v0.2.4 | MIT/Apache-2.0 |
| foreign-types-shared | v0.3.1 | MIT/Apache-2.0 |
| foreign-types | v0.5.0 | MIT/Apache-2.0 |
| futures-core | v0.3.34 | MIT OR Apache-2.0 |
| futures-task | v0.3.34 | MIT OR Apache-2.0 |
| futures-util | v0.3.34 | MIT OR Apache-2.0 |
| gethostname | v1.1.0 | Apache-2.0 |
| getrandom | v0.3.4 | MIT OR Apache-2.0 |
| gif | v0.14.2 | MIT OR Apache-2.0 |
| glam | v0.30.10 | MIT OR Apache-2.0 |
| glamx | v0.1.3 | MIT OR Apache-2.0 |
| glow | v0.17.0 | MIT OR Apache-2.0 OR Zlib |
| glutin_egl_sys | v0.7.1 | Apache-2.0 |
| glutin_glx_sys | v0.6.1 | Apache-2.0 |
| glutin | v0.32.3 | Apache-2.0 |
| glutin_wgl_sys | v0.6.1 | Apache-2.0 |
| glutin-winit | v0.5.0 | MIT |
| guillotiere | v0.7.0 | MIT/Apache-2.0 |
| harfrust | v0.12.0 | MIT |
| hash32 | v0.3.1 | MIT OR Apache-2.0 |
| hashbrown | v0.16.1 | MIT OR Apache-2.0 |
| hashbrown | v0.17.1 | MIT OR Apache-2.0 |
| heapless | v0.8.0 | MIT OR Apache-2.0 |
| hermit-abi | v0.5.3 | MIT OR Apache-2.0 |
| image | v0.25.10 | MIT OR Apache-2.0 |
| indexmap | v2.14.2 | Apache-2.0 OR MIT |
| itertools | v0.15.0 | MIT OR Apache-2.0 |
| itoa | v1.0.19 | MIT OR Apache-2.0 |
| jni-macros | v0.22.4 | MIT OR Apache-2.0 |
| jni-sys-macros | v0.4.1 | MIT OR Apache-2.0 |
| jni-sys | v0.3.1 | MIT OR Apache-2.0 |
| jni-sys | v0.4.1 | MIT OR Apache-2.0 |
| jni | v0.22.4 | MIT OR Apache-2.0 |
| js-sys | v0.3.107 | MIT OR Apache-2.0 |
| kurbo | v0.13.1 | Apache-2.0 OR MIT |
| libc | v0.2.191 | MIT OR Apache-2.0 |
| libloading | v0.8.9 | ISC |
| libm | v0.2.16 | MIT |
| libredox | v0.1.25 | MIT |
| linebender_resource_handle | v0.1.1 | Apache-2.0 OR MIT |
| linux-raw-sys | v0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| linux-raw-sys | v0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litrs | v1.0.1 | MIT OR Apache-2.0 |
| lock_api | v0.4.14 | MIT OR Apache-2.0 |
| log | v0.4.35 | MIT OR Apache-2.0 |
| matrixmultiply | v0.3.11 | MIT/Apache-2.0 |
| memchr | v2.8.3 | Unlicense OR MIT |
| memmap2 | v0.9.12 | MIT OR Apache-2.0 |
| miniz_oxide | v0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | v0.9.1 | MIT OR Zlib OR Apache-2.0 |
| moxcms | v0.8.1 | BSD-3-Clause OR Apache-2.0 |
| multiversion_no_op | v1.0.0 | Apache-2.0 OR MIT |
| nalgebra-macros | v0.3.0 | Apache-2.0 |
| nalgebra | v0.34.2 | Apache-2.0 |
| ndk-context | v0.1.1 | MIT OR Apache-2.0 |
| ndk-sys | v0.6.0+11769913 | MIT OR Apache-2.0 |
| ndk | v0.9.0 | MIT OR Apache-2.0 |
| nohash-hasher | v0.2.0 | Apache-2.0 OR MIT |
| num-complex | v0.4.6 | MIT OR Apache-2.0 |
| num-derive | v0.4.2 | MIT OR Apache-2.0 |
| num_enum_derive | v0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num_enum | v0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num-integer | v0.1.47 | MIT OR Apache-2.0 |
| num-rational | v0.4.2 | MIT OR Apache-2.0 |
| num-traits | v0.2.19 | MIT OR Apache-2.0 |
| objc2-app-kit | v0.2.2 | MIT |
| objc2-app-kit | v0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-foundation | v0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-encode | v4.1.0 | MIT |
| objc2-foundation | v0.2.2 | MIT |
| objc2-foundation | v0.3.2 | MIT |
| objc2-ui-kit | v0.2.2 | MIT |
| objc2-ui-kit | v0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2 | v0.5.3 | MIT |
| objc2 | v0.6.5 | MIT |
| objc-sys | v0.3.5 | MIT |
| once_cell | v1.21.4 | MIT OR Apache-2.0 |
| orbclient | v0.3.55 | MIT |
| ordered-float | v5.5.1 | MIT |
| parking_lot_core | v0.9.12 | MIT OR Apache-2.0 |
| parking_lot | v0.12.5 | MIT OR Apache-2.0 |
| parry3d | v0.26.1 | Apache-2.0 |
| paste | v1.0.15 | MIT OR Apache-2.0 |
| peniko | v0.6.1 | Apache-2.0 OR MIT |
| percent-encoding | v2.3.2 | MIT OR Apache-2.0 |
| pin-project-internal | v1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | v0.2.17 | Apache-2.0 OR MIT |
| pin-project | v1.1.13 | Apache-2.0 OR MIT |
| plain | v0.2.3 | MIT/Apache-2.0 |
| png | v0.18.1 | MIT OR Apache-2.0 |
| polling | v3.11.0 | Apache-2.0 OR MIT |
| pollster | v0.4.0 | Apache-2.0/MIT |
| polycool | v0.4.0 | MIT OR Apache-2.0 |
| proc-macro2 | v1.0.107 | MIT OR Apache-2.0 |
| proc-macro-crate | v3.5.0 | MIT OR Apache-2.0 |
| profiling-procmacros | v1.0.18 | MIT OR Apache-2.0 |
| profiling | v1.0.18 | MIT OR Apache-2.0 |
| pxfm | v0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-xml | v0.41.0 | MIT |
| quote | v1.0.47 | MIT OR Apache-2.0 |
| rapier3d | v0.32.0 | Apache-2.0 |
| rawpointer | v0.2.1 | MIT/Apache-2.0 |
| raw-window-handle | v0.6.2 | MIT OR Apache-2.0 OR Zlib |
| rayon-core | v1.13.0 | MIT OR Apache-2.0 |
| rayon | v1.12.0 | MIT OR Apache-2.0 |
| read-fonts | v0.41.0 | MIT OR Apache-2.0 |
| redox_syscall | v0.4.1 | MIT |
| redox_syscall | v0.5.18 | MIT |
| redox_syscall | v0.9.5 | MIT |
| r-efi | v5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rfd | v0.17.2 | MIT |
| robust | v1.2.0 | MIT OR Apache-2.0 |
| rstar | v0.12.2 | MIT OR Apache-2.0 |
| rustc-hash | v2.1.3 | Apache-2.0 OR MIT |
| rustix | v0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustix | v1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustversion | v1.0.23 | MIT OR Apache-2.0 |
| safe_arch | v0.7.4 | Zlib OR Apache-2.0 OR MIT |
| scoped-tls | v1.0.1 | MIT/Apache-2.0 |
| scopeguard | v1.2.0 | MIT OR Apache-2.0 |
| self_cell | v1.3.0 | Apache-2.0 OR GPL-2.0-only |
| send_wrapper | v0.6.0 | MIT/Apache-2.0 |
| serde_core | v1.0.229 | MIT OR Apache-2.0 |
| serde_derive | v1.0.229 | MIT OR Apache-2.0 |
| serde_json | v1.0.154 | MIT OR Apache-2.0 |
| serde | v1.0.229 | MIT OR Apache-2.0 |
| simba | v0.9.1 | Apache-2.0 |
| simd-adler32 | v0.3.10 | MIT |
| simd_cesu8 | v1.2.0 | Apache-2.0 OR MIT |
| simdutf8 | v0.1.5 | MIT OR Apache-2.0 |
| skrifa | v0.44.0 | MIT OR Apache-2.0 |
| slab | v0.4.12 | MIT |
| slotmap | v1.1.1 | Zlib |
| smallvec | v1.16.5 | MIT OR Apache-2.0 |
| smithay-client-toolkit | v0.19.2 | MIT |
| smol_str | v0.2.2 | MIT OR Apache-2.0 |
| spade | v2.15.1 | MIT OR Apache-2.0 |
| stable_deref_trait | v1.2.1 | MIT OR Apache-2.0 |
| static_assertions | v1.1.0 | MIT OR Apache-2.0 |
| syn | v2.0.119 | MIT OR Apache-2.0 |
| syn | v3.0.9 | MIT OR Apache-2.0 |
| thiserror-impl | v1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | v2.0.21 | MIT OR Apache-2.0 |
| thiserror | v1.0.69 | MIT OR Apache-2.0 |
| thiserror | v2.0.21 | MIT OR Apache-2.0 |
| tokio | v1.54.1 | MIT |
| toml_datetime | v1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | v0.25.19+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | v1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| tracing-core | v0.1.36 | MIT |
| tracing | v0.1.44 | MIT |
| typenum | v1.20.1 | MIT OR Apache-2.0 |
| unicode-general-category | v1.1.0 | Apache-2.0 |
| unicode-ident | v1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | v0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-segmentation | v1.13.3 | MIT OR Apache-2.0 |
| uuid | v1.29.2 | Apache-2.0 OR MIT |
| vec_map | v0.8.2 | MIT/Apache-2.0 |
| vello_common | v0.2.0 | Apache-2.0 OR MIT |
| vello_cpu | v0.2.0 | Apache-2.0 OR MIT |
| wasip2 | v1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen-futures | v0.4.80 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | v0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | v0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | v0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen | v0.2.130 | MIT OR Apache-2.0 |
| wayland-backend | v0.3.17 | MIT |
| wayland-client | v0.31.15 | MIT |
| wayland-csd-frame | v0.3.0 | MIT |
| wayland-cursor | v0.31.14 | MIT |
| wayland-protocols-plasma | v0.3.12 | MIT |
| wayland-protocols | v0.32.13 | MIT |
| wayland-protocols-wlr | v0.3.12 | MIT |
| wayland-scanner | v0.31.11 | MIT |
| wayland-sys | v0.31.11 | MIT |
| web-sys | v0.3.107 | MIT OR Apache-2.0 |
| web-time | v1.1.0 | MIT OR Apache-2.0 |
| weezl | v0.1.12 | MIT OR Apache-2.0 |
| wide | v0.7.33 | Zlib OR Apache-2.0 OR MIT |
| windows_aarch64_gnullvm | v0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | v0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | v0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | v0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | v0.52.6 | MIT OR Apache-2.0 |
| windows-link | v0.2.1 | MIT OR Apache-2.0 |
| windows-sys | v0.52.0 | MIT OR Apache-2.0 |
| windows-sys | v0.59.0 | MIT OR Apache-2.0 |
| windows-sys | v0.61.2 | MIT OR Apache-2.0 |
| windows-targets | v0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | v0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | v0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | v0.52.6 | MIT OR Apache-2.0 |
| winit | v0.30.13 | Apache-2.0 |
| winnow | v1.0.5 | MIT |
| wit-bindgen | v0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| x11-dl | v2.21.0 | MIT |
| x11rb-protocol | v0.13.2 | MIT OR Apache-2.0 |
| x11rb | v0.13.2 | MIT OR Apache-2.0 |
| xcursor | v0.3.11 | MIT |
| xkbcommon-dl | v0.4.2 | MIT |
| xkeysym | v0.2.1 | MIT OR Apache-2.0 OR Zlib |
| zerocopy-derive | v0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy | v0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zmij | v1.0.23 | MIT |
| zune-core | v0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | v0.5.15 | MIT OR Apache-2.0 OR Zlib |
