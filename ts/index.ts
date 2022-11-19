document.addEventListener("DOMContentLoaded", () => {});
document.addEventListener("gesturestart", (e) => e.preventDefault());

function registerDomButtonEventListeners(wasmModule: any) {
  const upButton = document.getElementsByClassName("up")[0];
  const downButton = document.getElementsByClassName("down")[0];
  const leftButton = document.getElementsByClassName("left")[0];
  const rightButton = document.getElementsByClassName("right")[0];

  for (const event of ["touchstart", "mousedown"]) {
    upButton.addEventListener(event, () => {
      console.log('up pressed');
      wasmModule.up_button_pressed();
    });
    downButton.addEventListener(event, () => {
      wasmModule.down_button_pressed();
    });
    leftButton.addEventListener(event, () => {
      wasmModule.left_button_pressed();
    });
    rightButton.addEventListener(event, () => {
      wasmModule.right_button_pressed();
    });
  }
  for (const event of ["touchend", "touchcancel", "mouseup", "mouseleave"]) {
    upButton.addEventListener(event, () => {
      console.log('up released');
      wasmModule.up_button_released();
    });
    downButton.addEventListener(event, () => {
      wasmModule.down_button_released();
    });
    leftButton.addEventListener(event, () => {
      wasmModule.left_button_released();
    });
    rightButton.addEventListener(event, () => {
      wasmModule.right_button_released();
    });
  }
}

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;

  registerDomButtonEventListeners(wasmModule);

  wasmModule.run(viewportWidth, viewportHeight);
});
