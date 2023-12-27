// HACK: convert to raw css using webpack
import controls from "./controls.scss";
controls;
import loader from "./loader.scss";
loader;
import { loadImage, cropImage } from "./blockDisplay";

import * as nipplejs from "nipplejs";
document.addEventListener("gesturestart", (e) => e.preventDefault());

/**
 * Determine the mobile operating system.
 * This function returns one of 'iOS', 'Android', 'Windows Phone', or 'unknown'.
 *
 * Source: https://stackoverflow.com/questions/21741841/detecting-ios-android-operating-system
 */
function getMobileOperatingSystem() {
  var userAgent =
    navigator.userAgent || navigator.vendor || (window as any).opera;

  // Windows Phone must come first because its UA also contains "Android"
  if (/windows phone/i.test(userAgent)) {
    return "Windows Phone";
  }

  if (/android/i.test(userAgent)) {
    return "Android";
  }

  // iOS detection from: http://stackoverflow.com/a/9039885/177710
  if (/iPad|iPhone|iPod/.test(userAgent) && !(window as any).MSStream) {
    return "iOS";
  }

  return "unknown";
}

let atlasImage: HTMLImageElement | null = null;

document.addEventListener("DOMContentLoaded", async () => {
  if (getMobileOperatingSystem() !== "unknown") {
    // Disable "mouse" events on game when on mobile
    document.getElementById("wasm-container").style.pointerEvents = "none";
  }

  atlasImage = await loadImage('./minecruft_atlas.png');
});

function isTouchDevice() {
  return (
    "ontouchstart" in window ||
    navigator.maxTouchPoints > 0 ||
    (navigator as any).msMaxTouchPoints > 0
  );
}

// Called from Rust code when the user chooses a different block to place
function handlePlaceBlockChanged(blockTypeStr: string) {
  if (!atlasImage) return;
  console.log("Block type changed to: " + blockTypeStr);

  let atlasIdxByBlockType: { [key: string]: [number, number] } = {
    "Dirt": [2, 0],
    "Stone": [2, 3],
    "Sand": [0, 1],
    "OakPlank": [2, 4],
    "Glass": [2, 1],
  };
  if (blockTypeStr in atlasIdxByBlockType) {
    let blockTypeIdx = atlasIdxByBlockType[blockTypeStr];
    let blockPreviewCanvas = cropImage(atlasImage, blockTypeIdx[0] * 16, blockTypeIdx[1] * 16, 16, 16);
    blockPreviewCanvas.id = "block-preview-canvas";
    document.getElementById("block-preview-canvas").replaceWith(blockPreviewCanvas);
  }
}
(window as any).handlePlaceBlockChanged = handlePlaceBlockChanged;

function registerDomButtonEventListeners(wasmModule: any) {
  const aButton = document.getElementById("a-button");
  const bButton = document.getElementById("b-button");
  const yButton = document.getElementById("y-button");

  const startEvent = isTouchDevice() ? "touchstart" : "mousedown";
  aButton.addEventListener(startEvent, () => {
    wasmModule.a_button_pressed();
  });
  bButton.addEventListener(startEvent, () => {
    wasmModule.b_button_pressed();
  });
  yButton.addEventListener(startEvent, () => {
    wasmModule.y_button_pressed();
  });
  for (const event of ["touchend", "touchcancel", "mouseup", "mouseleave"]) {
    aButton.addEventListener(event, () => {
      wasmModule.a_button_released();
    });
    bButton.addEventListener(event, () => {
      wasmModule.b_button_released();
    });
    yButton.addEventListener(event, () => {
      wasmModule.y_button_released();
    });
  }
}

// Ensure touches occur rapidly
const delay = 500;

// Track state of the last touch
let lastTapAt = 0;

export default function preventDoubleTapZoom(event: any) {
  // Exit early if this involves more than one finger (e.g. pinch to zoom)
  if (event.touches.length > 1) {
    return;
  }

  const tapAt = new Date().getTime();
  const timeDiff = tapAt - lastTapAt;
  if (event.touches.length === 1 && timeDiff < delay) {
    event.preventDefault();
    // Trigger a fake click for the tap we just prevented
    event.target.click();
  }
  lastTapAt = tapAt;
}

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

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
  pitchYawJoystickElem.addEventListener("touchstart", (event) =>
    preventDoubleTapZoom(event)
  );

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
  translationJoystickElem.addEventListener("touchstart", (event) =>
    preventDoubleTapZoom(event)
  );

  window.addEventListener("resize", () => {
    const viewportWidth = document.documentElement.clientWidth;
    const viewportHeight = document.documentElement.clientHeight;
    wasmModule.web_window_resized(viewportWidth, viewportHeight);
  });

  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;

  wasmModule.run(viewportWidth, viewportHeight);
}).catch((error) => {
  if (!error.message.startsWith("Using exceptions for control flow,")) {
    throw error;
  }
});
