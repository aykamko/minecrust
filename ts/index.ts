// HACK: convert to raw css using webpack
import controls from "./controls.scss";
controls;

import * as nipplejs from "nipplejs";
document.addEventListener("DOMContentLoaded", () => {});
document.addEventListener("gesturestart", (e) => e.preventDefault());

function isTouchDevice() {
  return (
    "ontouchstart" in window ||
    navigator.maxTouchPoints > 0 ||
    (navigator as any).msMaxTouchPoints > 0
  );
}

function registerDomButtonEventListeners(wasmModule: any) {
  const aButton = document.getElementById("a-button");
  const bButton = document.getElementById("b-button");

  const startEvent = isTouchDevice() ? "touchstart" : "mousedown";
  aButton.addEventListener(startEvent, () => {
    wasmModule.a_button_pressed();
  });
  bButton.addEventListener(startEvent, () => {
    wasmModule.b_button_pressed();
  });
  for (const event of ["touchend", "touchcancel", "mouseup", "mouseleave"]) {
    aButton.addEventListener(event, () => {
      console.log("up released");
      wasmModule.a_button_released();
    });
    bButton.addEventListener(event, () => {
      wasmModule.b_button_released();
    });
  }
}

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;

  registerDomButtonEventListeners(wasmModule);

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
