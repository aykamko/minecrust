document.addEventListener("DOMContentLoaded", () => {
    const upButton = document.getElementsByClassName("up")[0];
    const downButton = document.getElementsByClassName("up")[0];
    const leftButton = document.getElementsByClassName("up")[0];
    const rightButton = document.getElementsByClassName("up")[0];
});
document.addEventListener("gesturestart", (e) => e.preventDefault());

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

  wasmModule.run(
    document.documentElement.clientWidth,
    document.documentElement.clientHeight
  );
});
