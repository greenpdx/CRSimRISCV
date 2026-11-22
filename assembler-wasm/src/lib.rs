mod assembler;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn assemble(source: &str) -> JsValue {
    let result = assembler::assemble(source);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}
