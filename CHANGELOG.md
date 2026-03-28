# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed
- `AgentBuilder::launch_grpc()` now returns `Result<(), tonic::transport::Error>` instead of panicking
- `ControllerApp::launch()` now returns `Result<(), tonic::transport::Error>` instead of panicking
- `BotModelFactory::new_bot()` now returns `Option<ScriptedBot>` instead of panicking on script errors
- Remove unused `SimulationState::Stopping` variant and `SimulationStats::simulation_id` field
- Fix potential overflow in exponential backoff calculation using `saturating_pow`
- Remove unused `FunSignature` fields (`hash`, `args`) from bot registry

### Added
- Comprehensive rustdoc documentation for all public items (structs, enums, traits, fields)
- Crate-level documentation with architecture overview and quick-start example
- Module-level documentation for all top-level modules
- Tests for `SequentialIdGenerator` (5 tests covering sequential generation, reuse, shrink, compact)
- Tests for `SimulationState::is_aligned` (12 tests covering all state combinations)
- Tests for `BotDef` and `SimulationDef` builders
- Tests for `SimulationActor::normalize_count` (3 tests including edge cases)
- Controller actor integration test
- Extended varint tests (roundtrip, edge cases, overflow, encoding compactness)
- Improved README with badges, architecture diagram, feature list, and library usage examples

### Fixed
- Typo "batter" -> "better" in builder test comment
- Typo "At leas" -> "at least" in varint error message

## [0.2.0] 2024-12-21
### Added
- Add enter-state hooks
- Add controller actor
- Expose grpc server module

## [0.1.2] 2022-05-28