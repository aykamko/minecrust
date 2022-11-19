import("../pkg/index").then(wasm_module => {
    console.log("WASM Loaded");
    wasm_module.run();
});
