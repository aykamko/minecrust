// HACK: convert to raw css using webpack
import controls from "./controls.scss";
controls;

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

  const pitchYawJoystickElem = document.getElementById("pitch-yaw-joystick");
  const pitchYawJoystick = nipplejs.create({
    zone: pitchYawJoystickElem,
    mode: "static",
    position: { left: "50%", top: "50%" },
    color: "black",
  });
  pitchYawJoystick.on("move", function (_, data) {
    console.log(data.vector);
    wasmModule.pitch_yaw_joystick_moved(data.vector.x, -data.vector.y);
  });
  pitchYawJoystick.on("end", function (_, data) {
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
  translationJoystick.on("move", function (_, data) {
    wasmModule.translation_joystick_moved(data.vector.x, data.vector.y);
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
