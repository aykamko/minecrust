import * as nipplejs from "nipplejs";
document.addEventListener("DOMContentLoaded", () => {});
document.addEventListener("gesturestart", (e) => e.preventDefault());

function registerDomButtonEventListeners(wasmModule: any) {
  const upButton = document.getElementsByClassName("up")[0];
  const downButton = document.getElementsByClassName("down")[0];
  const leftButton = document.getElementsByClassName("left")[0];
  const rightButton = document.getElementsByClassName("right")[0];

  for (const event of ["touchstart", "mousedown"]) {
    upButton.addEventListener(event, () => {
      console.log("up pressed");
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
      console.log("up released");
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

  //   registerDomButtonEventListeners(wasmModule);

  const joystickElem = document.getElementById("joystick");
  const joystick = nipplejs.create({
    zone: joystickElem,
    mode: "static",
    position: { left: "50%", top: "50%" },
    color: "black",
  });
  joystick.on("move", function (_, data) {
    console.log(data.vector);
    wasmModule.pitch_yaw_joystick_moved(data.vector.x, -data.vector.y);
  });
  joystick.on("end", function (_, data) {
    wasmModule.pitch_yaw_joystick_released();
  });

  const translationJoystickElem = document.getElementById(
    "translation-joystick"
  );
  const translationJoystick = nipplejs.create({
    zone: translationJoystickElem,
    mode: "static",
    position: { left: "50%", top: "50%" },
    color: "black",
  });
  translationJoystick.on("dir", function (_, data) {
    // console.log(data.vector);
    let directionEnum = (
      {
        up: 0,
        right: 1,
        down: 2,
        left: 3,
      } as const
    )[data.direction.angle];
    wasmModule.translation_joystick_direction_changed(directionEnum);
  });
  translationJoystick.on("end", function (_, data) {
    wasmModule.translation_joystick_released();
  });
  // .on(
  //   "dir:up plain:up dir:left plain:left dir:down " +
  //     "plain:down dir:right plain:right",
  //   function (evt, data) {
  //     console.log(evt, data);
  //   }
  // )
  // .on("pressure", function (evt, data) {
  //   console.log(evt, data);
  // });

  wasmModule.run(viewportWidth, viewportHeight);
});
