# Changelog

## [0.4.0](https://github.com/drmartinwimmer/api-manifest/compare/api-manifest-v0.3.0...api-manifest-v0.4.0) (2026-10-19)


### Features

* add public macros and derive macros to library API manifest ([4235ec3](https://github.com/drmartinwimmer/api-manifest/commit/4235ec3179ef923830925de9915e7af495fc36f3))
* add public macros and derive macros to library API manifest ([95353c1](https://github.com/drmartinwimmer/api-manifest/commit/95353c101e2a403ad52f33b10e3c1ecfb951cfc0))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * api-manifest-derive bumped from 0.3.0 to 0.4.0

## [0.3.0](https://github.com/drmartinwimmer/api-manifest/compare/api-manifest-v0.2.1...api-manifest-v0.3.0) (2026-10-18)


### Features

* add ApiManifest::check_path and support workspace Cargo.toml fields ([b0475a7](https://github.com/drmartinwimmer/api-manifest/commit/b0475a76096cfd29f2fe7fec0a8388ea372379ce))
* add ApiManifest::check_path and support workspace Cargo.toml fields ([a0ffcbd](https://github.com/drmartinwimmer/api-manifest/commit/a0ffcbd3c5687bec0a5884827cbbe62013d9989e)), closes [#21](https://github.com/drmartinwimmer/api-manifest/issues/21)


### Bug Fixes

* replace markdown parsing with direct markdown diff and JSON manifest check ([ef01840](https://github.com/drmartinwimmer/api-manifest/commit/ef018408697c8b9499bb73970716bc93aeeca1c2))
* replace markdown parsing with direct markdown diff and JSON manifest check ([741ab24](https://github.com/drmartinwimmer/api-manifest/commit/741ab24092b6cb3e985afb3a179dac0f0deb4a1b))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * api-manifest-derive bumped from 0.2.1 to 0.3.0

## [0.2.1](https://github.com/drmartinwimmer/api-manifest/compare/api-manifest-v0.2.0...api-manifest-v0.2.1) (2026-10-18)


### Bug Fixes

* **ci:** include API.md in release PR sync diff check and update API.md version ([77e4b44](https://github.com/drmartinwimmer/api-manifest/commit/77e4b44381e69ce626ec5a8aa497d08ef7ad2700))
* **ci:** include API.md in release PR sync diff check and update API.md version ([65437b1](https://github.com/drmartinwimmer/api-manifest/commit/65437b12d690fe180dcc8ff28531307439f3d1bc))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * api-manifest-derive bumped from 0.2.0 to 0.2.1

## [0.2.0](https://github.com/drmartinwimmer/api-manifest/compare/api-manifest-v0.1.0...api-manifest-v0.2.0) (2026-10-18)


### Features

* **cli:** add CLI command and argument extractor ([38f57d0](https://github.com/drmartinwimmer/api-manifest/commit/38f57d0324b68f3e7e8c21bbf962821e46956906))
* **cli:** add CLI command and argument extractor ([2d422da](https://github.com/drmartinwimmer/api-manifest/commit/2d422dadb73dc773fc7f23c15dfa1826e89ff0ba))
* **config:** add ConfigFile trait and derive macro ([aafabee](https://github.com/drmartinwimmer/api-manifest/commit/aafabee88d7e6a7753310f4dc9dc758948a04f93))
* **config:** add ConfigFile trait and derive macro ([8eca927](https://github.com/drmartinwimmer/api-manifest/commit/8eca92789501446c89b377ea9a7e12cb2091e7c5))
* **config:** add configuration format extractor ([ae74c2e](https://github.com/drmartinwimmer/api-manifest/commit/ae74c2e574f7259126401b262a4a6ccd6f33095f))
* **config:** add configuration format extractor ([f2ba087](https://github.com/drmartinwimmer/api-manifest/commit/f2ba087c5f449f1e34c7a507132d378a9a85ae10))
* **config:** derive ConfigFile for CargoManifest and resolve nested config structs ([681227b](https://github.com/drmartinwimmer/api-manifest/commit/681227b75087f0a617eedae8a972d26066e16f49))
* **config:** derive ConfigFile for CargoManifest and resolve nested config structs ([194110e](https://github.com/drmartinwimmer/api-manifest/commit/194110e9bf028b167e176426d57ca0979f14ab9b))
* **core:** add manifest schema, Markdown/JSON serializer, drift auditor, and CLI commands ([2c129a7](https://github.com/drmartinwimmer/api-manifest/commit/2c129a7ad6c706fe043fca61822c46ff66506bf5))
* **core:** add manifest schema, Markdown/JSON serializer, drift auditor, and CLI commands ([55f9ad2](https://github.com/drmartinwimmer/api-manifest/commit/55f9ad21b92b479d8486a69591446a2067c77538))
* **http:** add HTTP service endpoint extractor ([0ef571a](https://github.com/drmartinwimmer/api-manifest/commit/0ef571ac5ae5e1bfe57e9faf7b07723963c8c752))
* **http:** add HTTP service endpoint extractor ([d76de2c](https://github.com/drmartinwimmer/api-manifest/commit/d76de2c72da2c87f5eb92cfc5b152b18bef1d337))
* initialize api-manifest library and binary crate ([c31e1a3](https://github.com/drmartinwimmer/api-manifest/commit/c31e1a39cf4e1471e8b500b67b0d1d2047b3d924))
* initialize api-manifest library and binary crate ([8da9e82](https://github.com/drmartinwimmer/api-manifest/commit/8da9e8235f753dbe39811b43a6ae11f8b3743870))
* **library:** add library API extractor and generate baseline API.md ([f507942](https://github.com/drmartinwimmer/api-manifest/commit/f507942333d6d7ea3086b07d338f2a889126f8f2))
* **library:** add library API extractor and generate baseline API.md ([14933b0](https://github.com/drmartinwimmer/api-manifest/commit/14933b0b9ed43a5b29f9e2f9a040e1c257829b71))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * api-manifest-derive bumped from 0.1.0 to 0.2.0
