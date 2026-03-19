# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.1.0](https://github.com/forkline/prl/tree/v0.1.0) - 2026-03-19

### Added

- Add multi-source promotion support ([55ac1bc](https://github.com/forkline/prl/commit/55ac1bca1ab9d577db9d4bed37b9c271ce32290e))
- Change promote to apply by default, add --confirm flag ([6d47c4d](https://github.com/forkline/prl/commit/6d47c4d883db2203e034ba08d7adb3fa834a1fec))
- Improve promote output with professional summary ([4ec32b5](https://github.com/forkline/prl/commit/4ec32b5367ae3debb2d83e134c99b8edc6006f57))
- Add simplified single-repo config with default_source/dest ([8f6fdfe](https://github.com/forkline/prl/commit/8f6fdfe0b07b3de0de0f1d007dc0f8fccba45ba1))
- Rename binary to prl, make promote the default command ([f853985](https://github.com/forkline/prl/commit/f853985f079015cdb6c824e8232fdf265294b1e5))
- Fix multi-source promotion for cross-repo scenarios ([d8822db](https://github.com/forkline/prl/commit/d8822dbd74e373821e1dba87657d02f5a1cb79a3))
- Support single-source cross-repo promotion ([2177d30](https://github.com/forkline/prl/commit/2177d30dacd429fa6c9e6aee384c0d15279496f0))
- Add gitignore support for faster file discovery ([c893390](https://github.com/forkline/prl/commit/c893390a9c2224554f7d93a6b7419e1c5b7a3c8b))
- Support multiple default sources for multi-source promotion ([671febe](https://github.com/forkline/prl/commit/671febe5c97cefe57dde7cddf3371026c2a3cca2))

### Fixed

- Rename master to main (#3) ([ee7ee88](https://github.com/forkline/prl/commit/ee7ee8856195dbef24b202da19f838e80ccfc397))
- Separate --explain output from JSON in versions merge ([42af5d1](https://github.com/forkline/prl/commit/42af5d1b90aa08fddec4f12f25c2a1647f9d42fd))
- Add --force flag to skip clean tree check ([e0ac1ab](https://github.com/forkline/prl/commit/e0ac1ab6e574c1f53cc46afc3edbc7e2352247bb))
- Properly display error messages instead of Debug format ([36d5eeb](https://github.com/forkline/prl/commit/36d5eebd5e4451d6a26fdc32204e709687154f2c))
- Correct charts denylist pattern and remove ANSI codes from logs ([fe56200](https://github.com/forkline/prl/commit/fe56200dd3da5c6cca7d37db080b682661d43dfa))
- Use println! instead of info! for styled output in diff ([13de339](https://github.com/forkline/prl/commit/13de339c2427fb8f1d30cb9c64f86ed7f0cbbcf9))
- Handle corrupted audit log gracefully with backup ([a047728](https://github.com/forkline/prl/commit/a04772888b53a71c96d5b78500ea135f0063963d))

### Documentation

- Add logo and update README with centered header ([553cc2b](https://github.com/forkline/prl/commit/553cc2baa9b664b7cec123af8dfb55a374eadcf5))
- Add white background to logo, change title to Promrail ([5fefc85](https://github.com/forkline/prl/commit/5fefc85a8d5dba546c3bcde581501b655babfcf5))

### Chore

- Remove vendor from release process ([b5e5c05](https://github.com/forkline/prl/commit/b5e5c053a44104057b0bf9407f1f6a5ce1060284))

### Refactor

- Replace tracing with fern+log for simpler logging ([884acfb](https://github.com/forkline/prl/commit/884acfb7649e75d14ca3f0b9d6e6747fa05a6d40))
- Update all references from promrail to prl ([700f490](https://github.com/forkline/prl/commit/700f49090177c3572e8e2a4eb269cd0949fca51b))
- Improve maintainability with extracted helpers ([67f38a6](https://github.com/forkline/prl/commit/67f38a691bab534884076af979d783f60aac1947))
- Extract create_audit_entry to reduce duplication ([a1b92c4](https://github.com/forkline/prl/commit/a1b92c4f8441046888a730a82d61bdccd8142eaa))
- Break down execute_multi_source into focused functions ([c9717cb](https://github.com/forkline/prl/commit/c9717cb07cc23aef3fbbf3df029bd5cd5194156c))

## [v0.0.0](https://github.com/forkline/prl/tree/v0.0.0) - 2026-03-17

### Added

- Implement promrail gitops promotion tool ([590c22e](https://github.com/forkline/prl/commit/590c22e98f790f04738c39910bfb92d9280c5894))
- Add cross-repo version extraction (Phase 1+2) ([1ae785e](https://github.com/forkline/prl/commit/1ae785e7f61cd9553aa02aae726749f5c4ca0094))
- Add snapshot, conflict detection, and config diff (Phase 3+4) ([4bd2568](https://github.com/forkline/prl/commit/4bd25681b8861f05637b46899a7b97a2427ba280))
- Add embedded configuration docs with derive macro ([c98b236](https://github.com/forkline/prl/commit/c98b2366371f769411c2ab637ae874dd735ebdaf))
- Add multi-source merge and promotion rules (Phase 5) ([1af1904](https://github.com/forkline/prl/commit/1af1904a1ee4bd010ea7c343ecf6bdb6e5b22601))

### Fixed

- Yamllint brace spacing in workflow ([378c368](https://github.com/forkline/prl/commit/378c368af731445ac3076f2aec27bd3245156d2d))

### Documentation

- Add Phase 5 multi-source promotion documentation ([db0160a](https://github.com/forkline/prl/commit/db0160ac5ddf74a5f7337f2cb42ad85cc4b827b8))
- Add workflow secrets documentation ([2c7a2db](https://github.com/forkline/prl/commit/2c7a2db908be363fbee39d170dc7c421a2468f3d))
- Remove crates.io publishing references ([3c39193](https://github.com/forkline/prl/commit/3c39193da0725685c4ed8e343c3cdd463afbe577))

### Refactor

- Improve code maintainability ([93472ce](https://github.com/forkline/prl/commit/93472ce3bf0255ee08c6dd36dcd7ebaa2f92f9d1))
