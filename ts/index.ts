document.addEventListener("DOMContentLoaded", () => {
});
document.addEventListener("gesturestart", (e) => e.preventDefault());

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;

  const upButton = document.getElementsByClassName("up")[0];
  const downButton = document.getElementsByClassName("down")[0];
  const leftButton = document.getElementsByClassName("left")[0];
  const rightButton = document.getElementsByClassName("right")[0];

  upButton.addEventListener("touchstart", () => {
    wasmModule.up_button_pressed();
  });
  upButton.addEventListener("mousedown", () => {
    wasmModule.up_button_pressed();
  });

  wasmModule.run(viewportWidth, viewportHeight);
});
