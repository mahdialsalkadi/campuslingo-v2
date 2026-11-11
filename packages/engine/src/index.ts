/** The same Rust domain crate runs natively and in this WASM module. */
export { default as initializeEngine, evaluateJson } from '../generated/campuslingo_domain.js';
