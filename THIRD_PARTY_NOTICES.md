# Third-Party Notices / 제3자 구성요소 고지

## 요약 (Korean summary)

- 이 파일은 다해(Dahae) 배포물에 들어가는 제3자 구성요소와 각 라이선스를 나열합니다.
- 범위: 백엔드 API 바이너리의 Rust crate, 앱 3종(customer, partner, console)의 Dart/Flutter 패키지, 컨테이너 기반 이미지, 글꼴.
- 각 구성요소는 자신의 라이선스를 따릅니다. 이 파일은 고지용이며 라이선스 조건을 바꾸지 않습니다. 영문 본문이 우선합니다.
- 라이선스 조건 자체는 [LICENSE](LICENSE)에 있습니다.

## Scope and method

- **Rust**: every crate in the normal (non-dev, non-build) dependency graph of the `api` binary (`backend/crates/api`, which includes `backend/crates/domain`), for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, the two image platforms in [backend/deny.toml](backend/deny.toml). Source of truth: [backend/Cargo.lock](backend/Cargo.lock), license as declared in `cargo metadata --format-version 1 --locked`. Proc-macro crates are listed although they run only at compile time (conservative).
- **Dart/Flutter**: every hosted package reachable from the `dependencies` (not `dev_dependencies`) of the three shipped apps [apps/customer](app/apps/customer/pubspec.yaml), [apps/partner](app/apps/partner/pubspec.yaml) and [apps/console](app/apps/console/pubspec.yaml), resolved against [app/pubspec.lock](app/pubspec.lock). Licenses are identified from each package's `LICENSE` file in the pub cache, because pub.dev packages do not declare an SPDX identifier. Test-only packages (`test_support`, `integration_test`, `flutter_test`) are not shipped and are not listed.
- **Containers, fonts and website assets**: the images in [deploy/docker-compose.yml](deploy/docker-compose.yml), [deploy/web.Dockerfile](deploy/web.Dockerfile), [backend/Dockerfile](backend/Dockerfile) and [deploy/wal/Dockerfile](deploy/wal/Dockerfile), the fonts in [app/packages/ui/assets/fonts](app/packages/ui/assets/fonts/), and the bundled files in [website/](website/).
- Every component keeps its own license. The "License" column is the identifier as declared by the upstream project (Rust), or as read from its LICENSE text (Dart). If you find a wrong identifier, open an issue.

## Container images and base layers

| Component | Where it ships | Reference | License | Upstream |
|---|---|---|---|---|
| Debian `trixie-slim` | Base of the API image ([backend/Dockerfile](backend/Dockerfile) line 11) | `debian:trixie-slim@sha256:a72b4433…` | Each Debian package under its own license (see `/usr/share/doc/<package>/copyright` in the image) | https://www.debian.org/legal/licenses/ |
| Caddy 2.11.9 | Web image: the server binary ([deploy/web.Dockerfile](deploy/web.Dockerfile) lines 35-46) | `caddy:2.11.9` | Apache-2.0 | https://github.com/caddyserver/caddy |
| Alpine Linux packages | Base of the `caddy:2.11.9` image (the Dockerfile runs `apk upgrade`) | `caddy:2.11.9` | Each Alpine package under its own license | https://alpinelinux.org/ |
| PostgreSQL 17 | Database container ([deploy/docker-compose.yml](deploy/docker-compose.yml) line 21) | `postgres:17` | PostgreSQL License | https://www.postgresql.org/about/licence/ |
| pgBackRest | Optional WAL archive image only ([deploy/wal/Dockerfile](deploy/wal/Dockerfile) lines 3-6) | Debian package on `postgres:17` | MIT | https://pgbackrest.org/ |
| Uptime Kuma 2.1.1 | Optional `monitoring` profile only ([deploy/docker-compose.yml](deploy/docker-compose.yml) lines 94-99) | `louislam/uptime-kuma:2.1.1` | MIT | https://github.com/louislam/uptime-kuma |
| Flutter SDK 3.47.9 (web engine, CanvasKit, skwasm, Material icons font) | Compiled into the web image's `/srv/app/` and `/srv/console/` ([deploy/web.Dockerfile](deploy/web.Dockerfile) lines 9-33) | `FLUTTER_VERSION=3.47.9` | BSD-3-Clause (Flutter, Dart); the Material icons font is CC BY 4.0, see [Fonts](#fonts) | https://github.com/flutter/flutter |

Build-stage images, not contained in the shipped images: `rust:1.99.1-trixie` ([backend/Dockerfile](backend/Dockerfile) line 3), `ghcr.io/cirruslabs/flutter` and `caddy:2.11.9-builder` ([deploy/web.Dockerfile](deploy/web.Dockerfile) lines 9 and 35).

Not enumerated here: the Go modules compiled into the Caddy binary (the `xcaddy` build in [deploy/web.Dockerfile](deploy/web.Dockerfile) lines 35-44 pins `golang.org/x/net` and `github.com/klauspost/compress`). Their licenses follow the Go module graph of Caddy 2.11.9 and are not listed in this file.

## Fonts

| Font | Files | Origin | License | Upstream |
|---|---|---|---|---|
| Dahae Sans (Regular, Medium, SemiBold, Bold, ExtraBold) | [app/packages/ui/assets/fonts/](app/packages/ui/assets/fonts/) `DahaeSans-*.otf` | Subset of Pretendard, Copyright (c) 2021 Kil Hyung-jin. Renamed to "Dahae Sans" because the OFL reserves the name "Pretendard" ([app/tool/font_subset.py](app/tool/font_subset.py) lines 1-4) | SIL Open Font License 1.1, text in [OFL.txt](app/packages/ui/assets/fonts/OFL.txt) | https://github.com/orioncactus/pretendard |
| Dahae Sans Full (Regular) | [app/packages/ui/assets/fonts/full/DahaeSansFull-Regular.otf](app/packages/ui/assets/fonts/full/DahaeSansFull-Regular.otf) | Same origin, a wider subset for rare Hangul syllables (listed in [app/packages/ui/pubspec.yaml](app/packages/ui/pubspec.yaml) lines 33-36) | SIL Open Font License 1.1 | https://github.com/orioncactus/pretendard |
| Fallback fonts for web text (Roboto and Noto families) | Fetched at image build time into `/srv/webfonts/` ([deploy/web.Dockerfile](deploy/web.Dockerfile) lines 14-20 for the fetch and line 56 for the copy), not stored in the repository | URL list read from the pinned Flutter engine | Roboto under Apache-2.0 (bundled text: Flutter SDK `roboto_license.txt`); Noto under SIL OFL 1.1 (per upstream, not checked against a bundled text) | https://fonts.google.com/ |
| Material Icons font (Flutter SDK) | Bundled in the Flutter web builds served from `/srv/app/` and `/srv/console/`; not stored in the repository | Flutter SDK 3.47.9, `material_fonts/materialicons-regular.otf` | CC BY 4.0 (Attribution 4.0 International), per `materialicons_license.txt` bundled with the SDK; attribution required | https://fonts.google.com/icons |
| Dahae Sans web subset (Regular, Bold, ExtraBold) | [website/assets/fonts/](website/assets/fonts/) `DahaeSans-*.woff2` | Pretendard subset via [scripts/build_fonts.py](scripts/build_fonts.py) | SIL Open Font License 1.1, see [NOTICE](website/assets/fonts/NOTICE) and [OFL.txt](website/assets/fonts/OFL.txt) | https://github.com/orioncactus/pretendard |

The OFL notice for Dahae Sans: "Copyright (c) 2021, Kil Hyung-jin (https://github.com/orioncactus/pretendard), with Reserved Font Name Pretendard." The full OFL text is in [OFL.txt](app/packages/ui/assets/fonts/OFL.txt). The same notice is in [NOTICE.txt](app/packages/ui/assets/fonts/NOTICE.txt).

## Website assets

| Component | Files | License | Upstream |
|---|---|---|---|
| Three.js, minified build (file header version string `170`) | [website/vendor/three.hero.min.js](website/vendor/three.hero.min.js) | MIT (`@license` header: Copyright 2010-2024 Three.js Authors) | https://threejs.org |

The website is served by the web image (`COPY website/ /srv/website/`, [deploy/web.Dockerfile](deploy/web.Dockerfile) line 53).

## Rust crates (backend API binary)

Rust crates: 245 (api binary, normal dependencies; union of x86_64 and aarch64 Linux). Groups sorted by count.

### MIT OR Apache-2.0 (122)

| Crate | Version | License | Source |
|---|---|---|---|
| aead | 0.5.2 | MIT OR Apache-2.0 | https://crates.io/crates/aead/0.5.2 |
| aes | 0.8.4 | MIT OR Apache-2.0 | https://crates.io/crates/aes/0.8.4 |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | https://crates.io/crates/allocator-api2/0.2.21 |
| argon2 | 0.5.3 | MIT OR Apache-2.0 | https://crates.io/crates/argon2/0.5.3 |
| base64 | 0.22.1 | MIT OR Apache-2.0 | https://crates.io/crates/base64/0.22.1 |
| base64 | 0.23.1 | MIT OR Apache-2.0 | https://crates.io/crates/base64/0.23.1 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | https://crates.io/crates/bitflags/2.13.2 |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | https://crates.io/crates/blake2/0.10.6 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | https://crates.io/crates/block-buffer/0.10.4 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | https://crates.io/crates/cfg-if/1.0.5 |
| chrono | 0.4.45 | MIT OR Apache-2.0 | https://crates.io/crates/chrono/0.4.45 |
| cipher | 0.4.4 | MIT OR Apache-2.0 | https://crates.io/crates/cipher/0.4.4 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | https://crates.io/crates/cpufeatures/0.2.17 |
| crc | 3.4.0 | MIT OR Apache-2.0 | https://crates.io/crates/crc/3.4.0 |
| crc-catalog | 2.5.0 | MIT OR Apache-2.0 | https://crates.io/crates/crc-catalog/2.5.0 |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 | https://crates.io/crates/crossbeam-queue/0.3.14 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | https://crates.io/crates/crossbeam-utils/0.8.23 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | https://crates.io/crates/crypto-common/0.1.7 |
| ctr | 0.9.2 | MIT OR Apache-2.0 | https://crates.io/crates/ctr/0.9.2 |
| deranged | 0.5.9 | MIT OR Apache-2.0 | https://crates.io/crates/deranged/0.5.9 |
| digest | 0.10.7 | MIT OR Apache-2.0 | https://crates.io/crates/digest/0.10.7 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | https://crates.io/crates/displaydoc/0.2.7 |
| either | 1.19.0 | MIT OR Apache-2.0 | https://crates.io/crates/either/1.19.0 |
| errno | 0.3.14 | MIT OR Apache-2.0 | https://crates.io/crates/errno/0.3.14 |
| etcetera | 0.8.0 | MIT OR Apache-2.0 | https://crates.io/crates/etcetera/0.8.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | https://crates.io/crates/form_urlencoded/1.2.2 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-channel/0.3.34 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-core/0.3.34 |
| futures-intrusive | 0.5.0 | MIT OR Apache-2.0 | https://crates.io/crates/futures-intrusive/0.5.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-io/0.3.34 |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-macro/0.3.34 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-sink/0.3.34 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-task/0.3.34 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | https://crates.io/crates/futures-util/0.3.34 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | https://crates.io/crates/getrandom/0.2.17 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | https://crates.io/crates/getrandom/0.3.4 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | https://crates.io/crates/getrandom/0.4.3 |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | https://crates.io/crates/hashbrown/0.15.5 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | https://crates.io/crates/hashbrown/0.17.1 |
| hashlink | 0.10.0 | MIT OR Apache-2.0 | https://crates.io/crates/hashlink/0.10.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 | https://crates.io/crates/heck/0.5.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 | https://crates.io/crates/hex/0.4.3 |
| hkdf | 0.12.4 | MIT OR Apache-2.0 | https://crates.io/crates/hkdf/0.12.4 |
| hmac | 0.12.1 | MIT OR Apache-2.0 | https://crates.io/crates/hmac/0.12.1 |
| home | 0.5.12 | MIT OR Apache-2.0 | https://crates.io/crates/home/0.5.12 |
| http | 1.5.0 | MIT OR Apache-2.0 | https://crates.io/crates/http/1.5.0 |
| httparse | 1.10.1 | MIT OR Apache-2.0 | https://crates.io/crates/httparse/1.10.1 |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | https://crates.io/crates/httpdate/1.0.3 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | https://crates.io/crates/iana-time-zone/0.1.65 |
| idna | 1.1.0 | MIT OR Apache-2.0 | https://crates.io/crates/idna/1.1.0 |
| inout | 0.1.4 | MIT OR Apache-2.0 | https://crates.io/crates/inout/0.1.4 |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | https://crates.io/crates/ipnet/2.12.2 |
| itoa | 1.0.19 | MIT OR Apache-2.0 | https://crates.io/crates/itoa/1.0.19 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | https://crates.io/crates/lazy_static/1.5.1 |
| libc | 0.2.191 | MIT OR Apache-2.0 | https://crates.io/crates/libc/0.2.191 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | https://crates.io/crates/lock_api/0.4.14 |
| log | 0.4.35 | MIT OR Apache-2.0 | https://crates.io/crates/log/0.4.35 |
| md-5 | 0.10.6 | MIT OR Apache-2.0 | https://crates.io/crates/md-5/0.10.6 |
| mime | 0.3.18 | MIT OR Apache-2.0 | https://crates.io/crates/mime/0.3.18 |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | https://crates.io/crates/num-bigint/0.4.8 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | https://crates.io/crates/num-conv/0.2.2 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | https://crates.io/crates/num-integer/0.1.47 |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | https://crates.io/crates/num-iter/0.1.46 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | https://crates.io/crates/num-traits/0.2.19 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | https://crates.io/crates/once_cell/1.21.4 |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | https://crates.io/crates/opaque-debug/0.3.1 |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | https://crates.io/crates/parking_lot/0.12.5 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | https://crates.io/crates/parking_lot_core/0.9.12 |
| password-hash | 0.5.0 | MIT OR Apache-2.0 | https://crates.io/crates/password-hash/0.5.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | https://crates.io/crates/percent-encoding/2.3.2 |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | https://crates.io/crates/powerfmt/0.2.1 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | https://crates.io/crates/ppv-lite86/0.2.21 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | https://crates.io/crates/proc-macro2/1.0.107 |
| quote | 1.0.47 | MIT OR Apache-2.0 | https://crates.io/crates/quote/1.0.47 |
| rand | 0.8.8 | MIT OR Apache-2.0 | https://crates.io/crates/rand/0.8.8 |
| rand | 0.9.5 | MIT OR Apache-2.0 | https://crates.io/crates/rand/0.9.5 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | https://crates.io/crates/rand_chacha/0.3.1 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | https://crates.io/crates/rand_chacha/0.9.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | https://crates.io/crates/rand_core/0.6.4 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | https://crates.io/crates/rand_core/0.9.5 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | https://crates.io/crates/regex-automata/0.4.18 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | https://crates.io/crates/regex-syntax/0.8.11 |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | https://crates.io/crates/reqwest/0.12.28 |
| rsa | 0.9.10 | MIT OR Apache-2.0 | https://crates.io/crates/rsa/0.9.10 |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | https://crates.io/crates/rustls-pki-types/1.15.1 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | https://crates.io/crates/scopeguard/1.2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 | https://crates.io/crates/serde/1.0.229 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | https://crates.io/crates/serde_core/1.0.229 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | https://crates.io/crates/serde_derive/1.0.229 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | https://crates.io/crates/serde_json/1.0.154 |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | https://crates.io/crates/serde_path_to_error/0.1.20 |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | https://crates.io/crates/sha1/0.10.7 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | https://crates.io/crates/sha2/0.10.9 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | https://crates.io/crates/signal-hook-registry/1.4.8 |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | https://crates.io/crates/simdutf8/0.1.5 |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | https://crates.io/crates/smallvec/1.16.5 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | https://crates.io/crates/socket2/0.6.5 |
| sqlx | 0.8.6 | MIT OR Apache-2.0 | https://crates.io/crates/sqlx/0.8.6 |
| sqlx-core | 0.8.6 | MIT OR Apache-2.0 | https://crates.io/crates/sqlx-core/0.8.6 |
| sqlx-macros | 0.8.6 | MIT OR Apache-2.0 | https://crates.io/crates/sqlx-macros/0.8.6 |
| sqlx-macros-core | 0.8.6 | MIT OR Apache-2.0 | https://crates.io/crates/sqlx-macros-core/0.8.6 |
| sqlx-postgres | 0.8.6 | MIT OR Apache-2.0 | https://crates.io/crates/sqlx-postgres/0.8.6 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | https://crates.io/crates/stable_deref_trait/1.2.1 |
| syn | 2.0.119 | MIT OR Apache-2.0 | https://crates.io/crates/syn/2.0.119 |
| syn | 3.0.10 | MIT OR Apache-2.0 | https://crates.io/crates/syn/3.0.10 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | https://crates.io/crates/thiserror/2.0.21 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | https://crates.io/crates/thiserror-impl/2.0.21 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | https://crates.io/crates/thread_local/1.1.10 |
| time | 0.3.55 | MIT OR Apache-2.0 | https://crates.io/crates/time/0.3.55 |
| time-core | 0.1.9 | MIT OR Apache-2.0 | https://crates.io/crates/time-core/0.1.9 |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | https://crates.io/crates/time-macros/0.2.32 |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | https://crates.io/crates/tokio-rustls/0.26.6 |
| tungstenite | 0.29.0 | MIT OR Apache-2.0 | https://crates.io/crates/tungstenite/0.29.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 | https://crates.io/crates/typenum/1.20.1 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | https://crates.io/crates/unicode-bidi/0.3.18 |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | https://crates.io/crates/universal-hash/0.5.1 |
| url | 2.5.8 | MIT OR Apache-2.0 | https://crates.io/crates/url/2.5.8 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | https://crates.io/crates/windows-link/0.2.1 |
| windows-sys | 0.48.0 | MIT OR Apache-2.0 | https://crates.io/crates/windows-sys/0.48.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | https://crates.io/crates/windows-sys/0.61.2 |
| windows-targets | 0.48.5 | MIT OR Apache-2.0 | https://crates.io/crates/windows-targets/0.48.5 |
| windows_x86_64_msvc | 0.48.5 | MIT OR Apache-2.0 | https://crates.io/crates/windows_x86_64_msvc/0.48.5 |

### MIT (41)

| Crate | Version | License | Source |
|---|---|---|---|
| atoi | 2.0.0 | MIT | https://crates.io/crates/atoi/2.0.0 |
| axum | 0.8.9 | MIT | https://crates.io/crates/axum/0.8.9 |
| axum-core | 0.5.6 | MIT | https://crates.io/crates/axum-core/0.5.6 |
| axum-macros | 0.5.1 | MIT | https://crates.io/crates/axum-macros/0.5.1 |
| bytes | 1.12.1 | MIT | https://crates.io/crates/bytes/1.12.1 |
| data-encoding | 2.11.1 | MIT | https://crates.io/crates/data-encoding/2.11.1 |
| dotenvy | 0.15.7 | MIT | https://crates.io/crates/dotenvy/0.15.7 |
| generic-array | 0.14.7 | MIT | https://crates.io/crates/generic-array/0.14.7 |
| http-body | 1.1.0 | MIT | https://crates.io/crates/http-body/1.1.0 |
| http-body-util | 0.1.5 | MIT | https://crates.io/crates/http-body-util/0.1.5 |
| hyper | 1.12.0 | MIT | https://crates.io/crates/hyper/1.12.0 |
| hyper-util | 0.1.21 | MIT | https://crates.io/crates/hyper-util/0.1.21 |
| jsonwebtoken | 11.1.0 | MIT | https://crates.io/crates/jsonwebtoken/11.1.0 |
| libm | 0.2.16 | MIT | https://crates.io/crates/libm/0.2.16 |
| matchers | 0.2.0 | MIT | https://crates.io/crates/matchers/0.2.0 |
| mio | 1.2.4 | MIT | https://crates.io/crates/mio/1.2.4 |
| multer | 3.1.0 | MIT | https://crates.io/crates/multer/3.1.0 |
| nu-ansi-term | 0.50.3 | MIT | https://crates.io/crates/nu-ansi-term/0.50.3 |
| pem | 3.0.6 | MIT | https://crates.io/crates/pem/3.0.6 |
| sharded-slab | 0.1.7 | MIT | https://crates.io/crates/sharded-slab/0.1.7 |
| slab | 0.4.12 | MIT | https://crates.io/crates/slab/0.4.12 |
| spin | 0.9.9 | MIT | https://crates.io/crates/spin/0.9.9 |
| synstructure | 0.14.0 | MIT | https://crates.io/crates/synstructure/0.14.0 |
| tokio | 1.54.1 | MIT | https://crates.io/crates/tokio/1.54.1 |
| tokio-macros | 2.7.2 | MIT | https://crates.io/crates/tokio-macros/2.7.2 |
| tokio-stream | 0.1.19 | MIT | https://crates.io/crates/tokio-stream/0.1.19 |
| tokio-tungstenite | 0.29.0 | MIT | https://crates.io/crates/tokio-tungstenite/0.29.0 |
| tokio-util | 0.7.20 | MIT | https://crates.io/crates/tokio-util/0.7.20 |
| tower | 0.5.3 | MIT | https://crates.io/crates/tower/0.5.3 |
| tower-http | 0.6.11 | MIT | https://crates.io/crates/tower-http/0.6.11 |
| tower-layer | 0.3.3 | MIT | https://crates.io/crates/tower-layer/0.3.3 |
| tower-service | 0.3.3 | MIT | https://crates.io/crates/tower-service/0.3.3 |
| tracing | 0.1.44 | MIT | https://crates.io/crates/tracing/0.1.44 |
| tracing-attributes | 0.1.31 | MIT | https://crates.io/crates/tracing-attributes/0.1.31 |
| tracing-core | 0.1.36 | MIT | https://crates.io/crates/tracing-core/0.1.36 |
| tracing-log | 0.2.0 | MIT | https://crates.io/crates/tracing-log/0.2.0 |
| tracing-serde | 0.2.0 | MIT | https://crates.io/crates/tracing-serde/0.2.0 |
| tracing-subscriber | 0.3.23 | MIT | https://crates.io/crates/tracing-subscriber/0.3.23 |
| try-lock | 0.2.5 | MIT | https://crates.io/crates/try-lock/0.2.5 |
| want | 0.3.2 | MIT | https://crates.io/crates/want/0.3.2 |
| zmij | 1.0.23 | MIT | https://crates.io/crates/zmij/1.0.23 |

### Apache-2.0 OR MIT (34)

| Crate | Version | License | Source |
|---|---|---|---|
| aes-gcm | 0.10.3 | Apache-2.0 OR MIT | https://crates.io/crates/aes-gcm/0.10.3 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | https://crates.io/crates/atomic-waker/1.1.2 |
| base16ct | 0.2.0 | Apache-2.0 OR MIT | https://crates.io/crates/base16ct/0.2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | https://crates.io/crates/base64ct/1.8.3 |
| const-oid | 0.9.6 | Apache-2.0 OR MIT | https://crates.io/crates/const-oid/0.9.6 |
| crypto-bigint | 0.5.5 | Apache-2.0 OR MIT | https://crates.io/crates/crypto-bigint/0.5.5 |
| der | 0.7.10 | Apache-2.0 OR MIT | https://crates.io/crates/der/0.7.10 |
| ecdsa | 0.16.9 | Apache-2.0 OR MIT | https://crates.io/crates/ecdsa/0.16.9 |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT | https://crates.io/crates/ed25519/2.2.3 |
| elliptic-curve | 0.13.8 | Apache-2.0 OR MIT | https://crates.io/crates/elliptic-curve/0.13.8 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | https://crates.io/crates/equivalent/1.0.3 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | https://crates.io/crates/event-listener/5.4.2 |
| ghash | 0.5.1 | Apache-2.0 OR MIT | https://crates.io/crates/ghash/0.5.1 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | https://crates.io/crates/idna_adapter/1.2.2 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | https://crates.io/crates/indexmap/2.14.2 |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | https://crates.io/crates/multiversion_no_op/1.0.0 |
| p256 | 0.13.2 | Apache-2.0 OR MIT | https://crates.io/crates/p256/0.13.2 |
| p384 | 0.13.1 | Apache-2.0 OR MIT | https://crates.io/crates/p384/0.13.1 |
| parking | 2.2.1 | Apache-2.0 OR MIT | https://crates.io/crates/parking/2.2.1 |
| pem-rfc7468 | 0.7.0 | Apache-2.0 OR MIT | https://crates.io/crates/pem-rfc7468/0.7.0 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | https://crates.io/crates/pin-project-lite/0.2.17 |
| pkcs1 | 0.7.5 | Apache-2.0 OR MIT | https://crates.io/crates/pkcs1/0.7.5 |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT | https://crates.io/crates/pkcs8/0.10.2 |
| polyval | 0.6.2 | Apache-2.0 OR MIT | https://crates.io/crates/polyval/0.6.2 |
| primeorder | 0.13.6 | Apache-2.0 OR MIT | https://crates.io/crates/primeorder/0.13.6 |
| rfc6979 | 0.4.0 | Apache-2.0 OR MIT | https://crates.io/crates/rfc6979/0.4.0 |
| sec1 | 0.7.3 | Apache-2.0 OR MIT | https://crates.io/crates/sec1/0.7.3 |
| signature | 2.2.0 | Apache-2.0 OR MIT | https://crates.io/crates/signature/2.2.0 |
| spki | 0.7.3 | Apache-2.0 OR MIT | https://crates.io/crates/spki/0.7.3 |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | https://crates.io/crates/utf8_iter/1.1.0 |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | https://crates.io/crates/utf_types/1.0.0 |
| uuid | 1.29.2 | Apache-2.0 OR MIT | https://crates.io/crates/uuid/1.29.2 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | https://crates.io/crates/zeroize/1.9.1 |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | https://crates.io/crates/zeroize_derive/1.5.0 |

### Unicode-3.0 (18)

| Crate | Version | License | Source |
|---|---|---|---|
| icu_collections | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_collections/2.4.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_locale_core/2.4.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_normalizer/2.4.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_normalizer_data/2.4.0 |
| icu_properties | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_properties/2.4.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_properties_data/2.4.0 |
| icu_provider | 2.4.0 | Unicode-3.0 | https://crates.io/crates/icu_provider/2.4.0 |
| litemap | 0.8.3 | Unicode-3.0 | https://crates.io/crates/litemap/0.8.3 |
| potential_utf | 0.1.6 | Unicode-3.0 | https://crates.io/crates/potential_utf/0.1.6 |
| tinystr | 0.8.5 | Unicode-3.0 | https://crates.io/crates/tinystr/0.8.5 |
| writeable | 0.6.4 | Unicode-3.0 | https://crates.io/crates/writeable/0.6.4 |
| yoke | 0.8.4 | Unicode-3.0 | https://crates.io/crates/yoke/0.8.4 |
| yoke-derive | 0.8.4 | Unicode-3.0 | https://crates.io/crates/yoke-derive/0.8.4 |
| zerofrom | 0.1.8 | Unicode-3.0 | https://crates.io/crates/zerofrom/0.1.8 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | https://crates.io/crates/zerofrom-derive/0.1.8 |
| zerotrie | 0.2.5 | Unicode-3.0 | https://crates.io/crates/zerotrie/0.2.5 |
| zerovec | 0.11.9 | Unicode-3.0 | https://crates.io/crates/zerovec/0.11.9 |
| zerovec-derive | 0.11.7 | Unicode-3.0 | https://crates.io/crates/zerovec-derive/0.11.7 |

### MIT/Apache-2.0 (7)

| Crate | Version | License | Source |
|---|---|---|---|
| core_detect | 1.0.0 | MIT/Apache-2.0 | https://crates.io/crates/core_detect/1.0.0 |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 | https://crates.io/crates/curve25519-dalek-derive/0.1.1 |
| ff | 0.13.1 | MIT/Apache-2.0 | https://crates.io/crates/ff/0.13.1 |
| group | 0.13.0 | MIT/Apache-2.0 | https://crates.io/crates/group/0.13.0 |
| num-bigint-dig | 0.8.6 | MIT/Apache-2.0 | https://crates.io/crates/num-bigint-dig/0.8.6 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | https://crates.io/crates/serde_urlencoded/0.7.1 |
| stringprep | 0.1.5 | MIT/Apache-2.0 | https://crates.io/crates/stringprep/0.1.5 |

### (MIT OR Apache-2.0) AND Unicode-3.0 (3)

| Crate | Version | License | Source |
|---|---|---|---|
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://crates.io/crates/unicode-ident/1.0.27 |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://crates.io/crates/unicode-normalization/0.1.26 |
| unicode-properties | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://crates.io/crates/unicode-properties/0.1.5 |

### BSD-3-Clause (3)

| Crate | Version | License | Source |
|---|---|---|---|
| curve25519-dalek | 4.1.3 | BSD-3-Clause | https://crates.io/crates/curve25519-dalek/4.1.3 |
| ed25519-dalek | 2.2.0 | BSD-3-Clause | https://crates.io/crates/ed25519-dalek/2.2.0 |
| subtle | 2.6.1 | BSD-3-Clause | https://crates.io/crates/subtle/2.6.1 |

### ISC (3)

| Crate | Version | License | Source |
|---|---|---|---|
| rustls-webpki | 0.103.16 | ISC | https://crates.io/crates/rustls-webpki/0.103.16 |
| simple_asn1 | 0.6.4 | ISC | https://crates.io/crates/simple_asn1/0.6.4 |
| untrusted | 0.9.0 | ISC | https://crates.io/crates/untrusted/0.9.0 |

### Unlicense OR MIT (2)

| Crate | Version | License | Source |
|---|---|---|---|
| byteorder | 1.5.0 | Unlicense OR MIT | https://crates.io/crates/byteorder/1.5.0 |
| memchr | 2.8.3 | Unlicense OR MIT | https://crates.io/crates/memchr/2.8.3 |

### Apache-2.0 OR ISC OR MIT (2)

| Crate | Version | License | Source |
|---|---|---|---|
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | https://crates.io/crates/hyper-rustls/0.27.10 |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | https://crates.io/crates/rustls/0.23.46 |

### (Apache-2.0 OR MIT) AND BSD-3-Clause (1)

| Crate | Version | License | Source |
|---|---|---|---|
| encoding_rs | 0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause | https://crates.io/crates/encoding_rs/0.8.44 |

### Apache-2.0 (1)

| Crate | Version | License | Source |
|---|---|---|---|
| sync_wrapper | 1.0.2 | Apache-2.0 | https://crates.io/crates/sync_wrapper/1.0.2 |

### Apache-2.0 AND ISC (1)

| Crate | Version | License | Source |
|---|---|---|---|
| ring | 0.17.14 | Apache-2.0 AND ISC | https://crates.io/crates/ring/0.17.14 |

### Apache-2.0 OR BSL-1.0 (1)

| Crate | Version | License | Source |
|---|---|---|---|
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://crates.io/crates/ryu/1.0.23 |

### Apache-2.0 OR BSL-1.0 OR MIT (1)

| Crate | Version | License | Source |
|---|---|---|---|
| whoami | 1.6.1 | Apache-2.0 OR BSL-1.0 OR MIT | https://crates.io/crates/whoami/1.6.1 |

### BSD-2-Clause OR Apache-2.0 OR MIT (1)

| Crate | Version | License | Source |
|---|---|---|---|
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | https://crates.io/crates/zerocopy/0.8.63 |

### CDLA-Permissive-2.0 (1)

| Crate | Version | License | Source |
|---|---|---|---|
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | https://crates.io/crates/webpki-roots/1.0.11 |

### MIT AND BSD-3-Clause (1)

| Crate | Version | License | Source |
|---|---|---|---|
| matchit | 0.8.4 | MIT AND BSD-3-Clause | https://crates.io/crates/matchit/0.8.4 |

### Zlib (1)

| Crate | Version | License | Source |
|---|---|---|---|
| foldhash | 0.1.5 | Zlib | https://crates.io/crates/foldhash/0.1.5 |

### Zlib OR Apache-2.0 OR MIT (1)

| Crate | Version | License | Source |
|---|---|---|---|
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | https://crates.io/crates/tinyvec/1.13.3 |

## Dart and Flutter packages (customer, partner, console apps)

Dart/Flutter packages: 151 hosted (pub.dev) plus the Flutter SDK packages below. Identifiers are read from each package's LICENSE file (pub.dev does not declare an SPDX id).

### Flutter SDK (BSD-3-Clause)

| Package | Source | License |
|---|---|---|
| flutter | Flutter SDK, https://github.com/flutter/flutter | BSD-3-Clause |
| flutter_localizations | Flutter SDK, https://github.com/flutter/flutter | BSD-3-Clause |
| flutter_web_plugins | Flutter SDK, https://github.com/flutter/flutter | BSD-3-Clause |

### BSD-3-Clause (110)

| Package | Version | License | Source |
|---|---|---|---|
| _flutterfire_internals | 1.3.79 | BSD-3-Clause | https://pub.dev/packages/_flutterfire_internals/versions/1.3.79 |
| args | 2.7.0 | BSD-3-Clause | https://pub.dev/packages/args/versions/2.7.0 |
| async | 2.13.1 | BSD-3-Clause | https://pub.dev/packages/async/versions/2.13.1 |
| characters | 1.4.1 | BSD-3-Clause | https://pub.dev/packages/characters/versions/1.4.1 |
| cli_util | 0.6.0 | BSD-3-Clause | https://pub.dev/packages/cli_util/versions/0.6.0 |
| code_assets | 2.2.0 | BSD-3-Clause | https://pub.dev/packages/code_assets/versions/2.2.0 |
| collection | 1.19.1 | BSD-3-Clause | https://pub.dev/packages/collection/versions/1.19.1 |
| convert | 3.1.2 | BSD-3-Clause | https://pub.dev/packages/convert/versions/3.1.2 |
| cross_file | 0.3.5+5 | BSD-3-Clause | https://pub.dev/packages/cross_file/versions/0.3.5+5 |
| crypto | 3.0.8 | BSD-3-Clause | https://pub.dev/packages/crypto/versions/3.0.8 |
| encrypt | 5.0.3 | BSD-3-Clause | https://pub.dev/packages/encrypt/versions/5.0.3 |
| ffi | 2.2.0 | BSD-3-Clause | https://pub.dev/packages/ffi/versions/2.2.0 |
| ffi_leak_tracker | 0.1.2 | BSD-3-Clause | https://pub.dev/packages/ffi_leak_tracker/versions/0.1.2 |
| file | 7.0.1 | BSD-3-Clause | https://pub.dev/packages/file/versions/7.0.1 |
| file_selector_linux | 0.9.4+1 | BSD-3-Clause | https://pub.dev/packages/file_selector_linux/versions/0.9.4+1 |
| file_selector_macos | 0.9.5+2 | BSD-3-Clause | https://pub.dev/packages/file_selector_macos/versions/0.9.5+2 |
| file_selector_platform_interface | 2.7.0 | BSD-3-Clause | https://pub.dev/packages/file_selector_platform_interface/versions/2.7.0 |
| file_selector_windows | 0.9.3+6 | BSD-3-Clause | https://pub.dev/packages/file_selector_windows/versions/0.9.3+6 |
| firebase_core | 5.1.0 | BSD-3-Clause | https://pub.dev/packages/firebase_core/versions/5.1.0 |
| firebase_core_platform_interface | 8.1.1 | BSD-3-Clause | https://pub.dev/packages/firebase_core_platform_interface/versions/8.1.1 |
| firebase_core_web | 4.1.0 | BSD-3-Clause | https://pub.dev/packages/firebase_core_web/versions/4.1.0 |
| firebase_messaging | 17.0.1 | BSD-3-Clause | https://pub.dev/packages/firebase_messaging/versions/17.0.1 |
| firebase_messaging_platform_interface | 4.11.1 | BSD-3-Clause | https://pub.dev/packages/firebase_messaging_platform_interface/versions/4.11.1 |
| firebase_messaging_web | 4.3.1 | BSD-3-Clause | https://pub.dev/packages/firebase_messaging_web/versions/4.3.1 |
| fixnum | 1.1.1 | BSD-3-Clause | https://pub.dev/packages/fixnum/versions/1.1.1 |
| flutter_animate | 4.5.2 | BSD-3-Clause | https://pub.dev/packages/flutter_animate/versions/4.5.2 |
| flutter_local_notifications | 22.3.3 | BSD-3-Clause | https://pub.dev/packages/flutter_local_notifications/versions/22.3.3 |
| flutter_local_notifications_linux | 8.0.2 | BSD-3-Clause | https://pub.dev/packages/flutter_local_notifications_linux/versions/8.0.2 |
| flutter_local_notifications_platform_interface | 12.2.0 | BSD-3-Clause | https://pub.dev/packages/flutter_local_notifications_platform_interface/versions/12.2.0 |
| flutter_local_notifications_web | 1.0.0 | BSD-3-Clause | https://pub.dev/packages/flutter_local_notifications_web/versions/1.0.0 |
| flutter_local_notifications_windows | 3.1.1 | BSD-3-Clause | https://pub.dev/packages/flutter_local_notifications_windows/versions/3.1.1 |
| flutter_plugin_android_lifecycle | 2.0.35 | BSD-3-Clause | https://pub.dev/packages/flutter_plugin_android_lifecycle/versions/2.0.35 |
| flutter_secure_storage | 11.2.0 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage/versions/11.2.0 |
| flutter_secure_storage_darwin | 0.4.3 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage_darwin/versions/0.4.3 |
| flutter_secure_storage_linux | 3.0.3 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage_linux/versions/3.0.3 |
| flutter_secure_storage_platform_interface | 2.1.1 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage_platform_interface/versions/2.1.1 |
| flutter_secure_storage_web | 2.1.1 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage_web/versions/2.1.1 |
| flutter_secure_storage_windows | 4.2.2 | BSD-3-Clause | https://pub.dev/packages/flutter_secure_storage_windows/versions/4.2.2 |
| flutter_shaders | 0.1.5 | BSD-3-Clause | https://pub.dev/packages/flutter_shaders/versions/0.1.5 |
| google_identity_services_web | 0.3.3+1 | BSD-3-Clause | https://pub.dev/packages/google_identity_services_web/versions/0.3.3+1 |
| google_sign_in | 7.2.0 | BSD-3-Clause | https://pub.dev/packages/google_sign_in/versions/7.2.0 |
| google_sign_in_android | 7.2.18 | BSD-3-Clause | https://pub.dev/packages/google_sign_in_android/versions/7.2.18 |
| google_sign_in_ios | 6.4.0 | BSD-3-Clause | https://pub.dev/packages/google_sign_in_ios/versions/6.4.0 |
| google_sign_in_platform_interface | 3.1.0 | BSD-3-Clause | https://pub.dev/packages/google_sign_in_platform_interface/versions/3.1.0 |
| google_sign_in_web | 1.1.3 | BSD-3-Clause | https://pub.dev/packages/google_sign_in_web/versions/1.1.3 |
| hooks | 2.2.0 | BSD-3-Clause | https://pub.dev/packages/hooks/versions/2.2.0 |
| http | 1.6.0 | BSD-3-Clause | https://pub.dev/packages/http/versions/1.6.0 |
| http_parser | 4.1.2 | BSD-3-Clause | https://pub.dev/packages/http_parser/versions/4.1.2 |
| image_picker_for_web | 3.1.1 | BSD-3-Clause | https://pub.dev/packages/image_picker_for_web/versions/3.1.1 |
| image_picker_linux | 0.2.2 | BSD-3-Clause | https://pub.dev/packages/image_picker_linux/versions/0.2.2 |
| image_picker_macos | 0.2.2+1 | BSD-3-Clause | https://pub.dev/packages/image_picker_macos/versions/0.2.2+1 |
| image_picker_platform_interface | 2.11.2 | BSD-3-Clause | https://pub.dev/packages/image_picker_platform_interface/versions/2.11.2 |
| image_picker_windows | 0.2.2 | BSD-3-Clause | https://pub.dev/packages/image_picker_windows/versions/0.2.2 |
| intl | 0.20.3 | BSD-3-Clause | https://pub.dev/packages/intl/versions/0.20.3 |
| jni | 1.1.2 | BSD-3-Clause | https://pub.dev/packages/jni/versions/1.1.2 |
| jni_flutter | 1.0.4+1 | BSD-3-Clause | https://pub.dev/packages/jni_flutter/versions/1.0.4+1 |
| jni_util | 1.0.0 | BSD-3-Clause | https://pub.dev/packages/jni_util/versions/1.0.0 |
| js | 0.7.2 | BSD-3-Clause | https://pub.dev/packages/js/versions/0.7.2 |
| json_annotation | 4.12.0 | BSD-3-Clause | https://pub.dev/packages/json_annotation/versions/4.12.0 |
| logging | 1.3.0 | BSD-3-Clause | https://pub.dev/packages/logging/versions/1.3.0 |
| meta | 1.19.0 | BSD-3-Clause | https://pub.dev/packages/meta/versions/1.19.0 |
| mime | 2.1.0 | BSD-3-Clause | https://pub.dev/packages/mime/versions/2.1.0 |
| objective_c | 9.6.3 | BSD-3-Clause | https://pub.dev/packages/objective_c/versions/9.6.3 |
| package_config | 3.0.0 | BSD-3-Clause | https://pub.dev/packages/package_config/versions/3.0.0 |
| package_info_plus | 10.2.2 | BSD-3-Clause | https://pub.dev/packages/package_info_plus/versions/10.2.2 |
| package_info_plus_platform_interface | 4.1.0 | BSD-3-Clause | https://pub.dev/packages/package_info_plus_platform_interface/versions/4.1.0 |
| path | 1.9.1 | BSD-3-Clause | https://pub.dev/packages/path/versions/1.9.1 |
| path_provider | 2.1.6 | BSD-3-Clause | https://pub.dev/packages/path_provider/versions/2.1.6 |
| path_provider_android | 2.3.1 | BSD-3-Clause | https://pub.dev/packages/path_provider_android/versions/2.3.1 |
| path_provider_foundation | 2.6.0 | BSD-3-Clause | https://pub.dev/packages/path_provider_foundation/versions/2.6.0 |
| path_provider_linux | 2.2.2 | BSD-3-Clause | https://pub.dev/packages/path_provider_linux/versions/2.2.2 |
| path_provider_platform_interface | 2.1.3 | BSD-3-Clause | https://pub.dev/packages/path_provider_platform_interface/versions/2.1.3 |
| path_provider_windows | 2.3.0 | BSD-3-Clause | https://pub.dev/packages/path_provider_windows/versions/2.3.0 |
| platform | 3.2.0 | BSD-3-Clause | https://pub.dev/packages/platform/versions/3.2.0 |
| plugin_platform_interface | 2.1.8 | BSD-3-Clause | https://pub.dev/packages/plugin_platform_interface/versions/2.1.8 |
| pub_semver | 2.2.1 | BSD-3-Clause | https://pub.dev/packages/pub_semver/versions/2.2.1 |
| record_use | 1.1.1 | BSD-3-Clause | https://pub.dev/packages/record_use/versions/1.1.1 |
| shared_preferences | 2.5.6 | BSD-3-Clause | https://pub.dev/packages/shared_preferences/versions/2.5.6 |
| shared_preferences_android | 2.5.4 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_android/versions/2.5.4 |
| shared_preferences_foundation | 2.6.1 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_foundation/versions/2.6.1 |
| shared_preferences_linux | 2.4.1 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_linux/versions/2.4.1 |
| shared_preferences_platform_interface | 2.4.2 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_platform_interface/versions/2.4.2 |
| shared_preferences_web | 2.4.3 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_web/versions/2.4.3 |
| shared_preferences_windows | 2.4.1 | BSD-3-Clause | https://pub.dev/packages/shared_preferences_windows/versions/2.4.1 |
| source_span | 1.10.2 | BSD-3-Clause | https://pub.dev/packages/source_span/versions/1.10.2 |
| stream_channel | 2.1.4 | BSD-3-Clause | https://pub.dev/packages/stream_channel/versions/2.1.4 |
| string_scanner | 1.4.1 | BSD-3-Clause | https://pub.dev/packages/string_scanner/versions/1.4.1 |
| term_glyph | 1.2.2 | BSD-3-Clause | https://pub.dev/packages/term_glyph/versions/1.2.2 |
| typed_data | 1.4.0 | BSD-3-Clause | https://pub.dev/packages/typed_data/versions/1.4.0 |
| url_launcher | 6.3.3 | BSD-3-Clause | https://pub.dev/packages/url_launcher/versions/6.3.3 |
| url_launcher_android | 6.3.33 | BSD-3-Clause | https://pub.dev/packages/url_launcher_android/versions/6.3.33 |
| url_launcher_ios | 6.4.2 | BSD-3-Clause | https://pub.dev/packages/url_launcher_ios/versions/6.4.2 |
| url_launcher_linux | 3.2.3 | BSD-3-Clause | https://pub.dev/packages/url_launcher_linux/versions/3.2.3 |
| url_launcher_macos | 3.2.6 | BSD-3-Clause | https://pub.dev/packages/url_launcher_macos/versions/3.2.6 |
| url_launcher_platform_interface | 2.3.2 | BSD-3-Clause | https://pub.dev/packages/url_launcher_platform_interface/versions/2.3.2 |
| url_launcher_web | 2.4.3 | BSD-3-Clause | https://pub.dev/packages/url_launcher_web/versions/2.4.3 |
| url_launcher_windows | 3.1.6 | BSD-3-Clause | https://pub.dev/packages/url_launcher_windows/versions/3.1.6 |
| vector_graphics | 1.2.6 | BSD-3-Clause | https://pub.dev/packages/vector_graphics/versions/1.2.6 |
| vector_graphics_codec | 1.1.13 | BSD-3-Clause | https://pub.dev/packages/vector_graphics_codec/versions/1.1.13 |
| vector_graphics_compiler | 1.3.5 | BSD-3-Clause | https://pub.dev/packages/vector_graphics_compiler/versions/1.3.5 |
| vector_math | 2.4.4 | BSD-3-Clause | https://pub.dev/packages/vector_math/versions/2.4.4 |
| web | 1.1.2 | BSD-3-Clause | https://pub.dev/packages/web/versions/1.1.2 |
| web_socket | 1.0.1 | BSD-3-Clause | https://pub.dev/packages/web_socket/versions/1.0.1 |
| web_socket_channel | 3.0.3 | BSD-3-Clause | https://pub.dev/packages/web_socket_channel/versions/3.0.3 |
| webview_flutter | 4.14.1 | BSD-3-Clause | https://pub.dev/packages/webview_flutter/versions/4.14.1 |
| webview_flutter_android | 4.14.1 | BSD-3-Clause | https://pub.dev/packages/webview_flutter_android/versions/4.14.1 |
| webview_flutter_platform_interface | 2.15.1 | BSD-3-Clause | https://pub.dev/packages/webview_flutter_platform_interface/versions/2.15.1 |
| webview_flutter_wkwebview | 3.27.2 | BSD-3-Clause | https://pub.dev/packages/webview_flutter_wkwebview/versions/3.27.2 |
| win32 | 6.4.0 | BSD-3-Clause | https://pub.dev/packages/win32/versions/6.4.0 |
| xdg_directories | 1.1.0 | BSD-3-Clause | https://pub.dev/packages/xdg_directories/versions/1.1.0 |

### MIT (29)

| Package | Version | License | Source |
|---|---|---|---|
| dio | 5.11.1 | MIT | https://pub.dev/packages/dio/versions/5.11.1 |
| dio_web_adapter | 2.2.2 | MIT | https://pub.dev/packages/dio_web_adapter/versions/2.2.2 |
| equatable | 3.0.0 | MIT | https://pub.dev/packages/equatable/versions/3.0.0 |
| fl_chart | 1.3.1 | MIT | https://pub.dev/packages/fl_chart/versions/1.3.1 |
| flutter_svg | 2.3.2 | MIT | https://pub.dev/packages/flutter_svg/versions/2.3.2 |
| flutter_web_auth_2 | 5.1.0 | MIT | https://pub.dev/packages/flutter_web_auth_2/versions/5.1.0 |
| flutter_web_auth_2_platform_interface | 5.0.0 | MIT | https://pub.dev/packages/flutter_web_auth_2_platform_interface/versions/5.0.0 |
| geocoding | 5.0.1 | MIT | https://pub.dev/packages/geocoding/versions/5.0.1 |
| geocoding_android | 5.1.0 | MIT | https://pub.dev/packages/geocoding_android/versions/5.1.0 |
| geocoding_darwin | 1.0.3 | MIT | https://pub.dev/packages/geocoding_darwin/versions/1.0.3 |
| geocoding_platform_interface | 5.0.0 | MIT | https://pub.dev/packages/geocoding_platform_interface/versions/5.0.0 |
| geolocator | 14.0.1 | MIT | https://pub.dev/packages/geolocator/versions/14.0.1 |
| geolocator_android | 5.2.1 | MIT | https://pub.dev/packages/geolocator_android/versions/5.2.1 |
| geolocator_apple | 2.4.3 | MIT | https://pub.dev/packages/geolocator_apple/versions/2.4.3 |
| geolocator_platform_interface | 4.4.1 | MIT | https://pub.dev/packages/geolocator_platform_interface/versions/4.4.1 |
| geolocator_web | 4.1.4 | MIT | https://pub.dev/packages/geolocator_web/versions/4.1.4 |
| geolocator_windows | 0.2.5 | MIT | https://pub.dev/packages/geolocator_windows/versions/0.2.5 |
| in_app_review | 2.0.12 | MIT | https://pub.dev/packages/in_app_review/versions/2.0.12 |
| in_app_review_platform_interface | 2.0.5 | MIT | https://pub.dev/packages/in_app_review_platform_interface/versions/2.0.5 |
| path_parsing | 1.1.0 | MIT | https://pub.dev/packages/path_parsing/versions/1.1.0 |
| petitparser | 7.1.0 | MIT | https://pub.dev/packages/petitparser/versions/7.1.0 |
| pointycastle | 3.9.1 | MIT | https://pub.dev/packages/pointycastle/versions/3.9.1 |
| sign_in_with_apple | 8.2.0 | MIT | https://pub.dev/packages/sign_in_with_apple/versions/8.2.0 |
| sign_in_with_apple_platform_interface | 2.0.0 | MIT | https://pub.dev/packages/sign_in_with_apple_platform_interface/versions/2.0.0 |
| sign_in_with_apple_web | 3.0.0 | MIT | https://pub.dev/packages/sign_in_with_apple_web/versions/3.0.0 |
| uuid | 4.6.0 | MIT | https://pub.dev/packages/uuid/versions/4.6.0 |
| window_to_front | 0.0.4 | MIT | https://pub.dev/packages/window_to_front/versions/0.0.4 |
| xml | 7.0.1 | MIT | https://pub.dev/packages/xml/versions/7.0.1 |
| yaml | 3.1.4 | MIT | https://pub.dev/packages/yaml/versions/3.1.4 |

### Apache-2.0 (9)

| Package | Version | License | Source |
|---|---|---|---|
| clock | 1.1.3 | Apache-2.0 | https://pub.dev/packages/clock/versions/1.1.3 |
| desktop_webview_window | 0.3.2 | Apache-2.0 | https://pub.dev/packages/desktop_webview_window/versions/0.3.2 |
| image_picker | 1.2.4 | Apache-2.0 | https://pub.dev/packages/image_picker/versions/1.2.4 |
| image_picker_android | 0.8.13+25 | Apache-2.0 | https://pub.dev/packages/image_picker_android/versions/0.8.13+25 |
| image_picker_ios | 0.8.13+15 | Apache-2.0 | https://pub.dev/packages/image_picker_ios/versions/0.8.13+15 |
| kakao_flutter_sdk_auth | 2.1.0 | Apache-2.0 | https://pub.dev/packages/kakao_flutter_sdk_auth/versions/2.1.0 |
| kakao_flutter_sdk_common | 2.1.0 | Apache-2.0 | https://pub.dev/packages/kakao_flutter_sdk_common/versions/2.1.0 |
| kakao_flutter_sdk_user | 2.1.0 | Apache-2.0 | https://pub.dev/packages/kakao_flutter_sdk_user/versions/2.1.0 |
| material_color_utilities | 0.13.0 | Apache-2.0 | https://pub.dev/packages/material_color_utilities/versions/0.13.0 |

### BSD-2-Clause (2)

| Package | Version | License | Source |
|---|---|---|---|
| asn1lib | 1.6.8 | BSD-2-Clause | https://pub.dev/packages/asn1lib/versions/1.6.8 |
| timezone | 0.11.2 | BSD-2-Clause | https://pub.dev/packages/timezone/versions/0.11.2 |

### MPL-2.0 (1)

| Package | Version | License | Source |
|---|---|---|---|
| dbus | 0.8.0 | MPL-2.0 | https://pub.dev/packages/dbus/versions/0.8.0 |

## Notes on the lists

- **Dual and multi-license crates.** Crates that offer a choice (for example `MIT OR Apache-2.0`) are listed with the expression as declared. Crates whose declared identifier is not a formal SPDX expression (`MIT/Apache-2.0`, `Apache-2.0/MIT`) are listed as written by the crate. The license chosen for use is the upstream's choice; this file does not pick one.
- **Copyleft in the shipped code.** The Rust graph has no copyleft license: the policy in [backend/deny.toml](backend/deny.toml) (`[licenses] allow`) permits only permissive licenses, and CI enforces it with `cargo deny`. The Dart graph contains one MPL-2.0 package, `dbus`, which is file-level copyleft. `dbus` is used only by the Linux desktop builds (via `flutter_local_notifications_linux`, [app/packages/core/pubspec.yaml](app/packages/core/pubspec.yaml) line 27); it is not in the web images. It is used as shipped and not modified in this repository. If it is redistributed, the MPL-2.0 applies to the dbus source files.
- **Flutter SDK packages.** `flutter`, `flutter_localizations` and `flutter_web_plugins` are part of the SDK, not hosted packages. The SDK is BSD-3-Clause.
- **Operating-system packages in images.** The Debian, Alpine and PostgreSQL images carry many packages with their own licenses. This file names the base images only; each image's own package metadata is the authority for those packages.

## Keeping this file current

When `backend/Cargo.lock` or `app/pubspec.lock` changes, regenerate the two tables from the same sources as above and review new license identifiers before release. The [supply-chain CI job](.github/workflows/ci.yml) already blocks disallowed Rust licenses; Dart licenses are not checked by CI.
