import main from "./main.scss";
main;
import loader from "./loader.scss";
loader;
import { loadImage, cropImage } from "./blockDisplay";

import * as nipplejs from "nipplejs";

document.addEventListener("gesturestart", (e) => e.preventDefault());

const hasChromeAgent = navigator.userAgent.indexOf("Chrome") > -1;
const hasSafariAgent = navigator.userAgent.indexOf("Safari") > -1;
const isSafari = hasSafariAgent && !hasChromeAgent;

function isTouchDevice() {
  return (
    "ontouchstart" in window ||
    navigator.maxTouchPoints > 0 ||
    (navigator as any).msMaxTouchPoints > 0
  );
}

const HIDE_JOYSTICK_HINTS_AFTER_N_JUMPS = 3;
function hideJoystickHints() {
  const joystickHints = document.getElementsByClassName("joystick-hint");
  for (const joystickHint of joystickHints) {
    (joystickHint as any).style.display = "none";
  }
}

// Disable right-click menu
document.addEventListener("contextmenu", (event: any) => {
  event.preventDefault();
});

let atlasImage: HTMLImageElement | null = null;

document.addEventListener("DOMContentLoaded", async () => {
  (window as any).feather?.replace();

  const controlsInfoPopup = document.getElementById("controls-info-popup");
  const wasmContainer = document.getElementById("wasm-container")
  if (isTouchDevice()) {
    // Disable "mouse" events on game when on mobile
    wasmContainer.style.pointerEvents = "none";
    controlsInfoPopup.style.display = "none";
  } else {
    // Hide button controls on desktop
    // TODO(aleks): this will probably break on touchscreen laptops, but oh well
    const buttonContainer = document.getElementsByClassName("button-container");
    (buttonContainer[0] as any).style.display = "none";
    const joystickDivs = document.getElementsByClassName("joystick");
    for (const joystickDiv of joystickDivs) {
      (joystickDiv as any).style.visibility = "hidden";
    }
  }

  atlasImage = await loadImage('./minecruft_atlas.png');
});

// Called from Rust code when the user chooses a different block to place
function handlePlaceBlockTypeChanged(blockTypeStr: string) {
  if (!atlasImage) return;
  // console.log("Block type changed to: " + blockTypeStr);

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
(window as any).handlePlaceBlockTypeChanged = handlePlaceBlockTypeChanged;

function registerDomButtonEventListeners(wasmModule: any) {
  const aButton = document.getElementById("a-button");
  const bButton = document.getElementById("b-button");
  const yButton = document.getElementById("y-button");
  const blockPreviewBtn = document.getElementById("block-preview");

  const startEvent = isTouchDevice() ? "touchstart" : "mousedown";
  aButton.addEventListener(startEvent, () => {
    wasmModule.a_button_pressed();
  });
  bButton.addEventListener(startEvent, () => {
    wasmModule.b_button_pressed();
  });
  // yButton.addEventListener(startEvent, () => {
  //   wasmModule.y_button_pressed();
  // });
  blockPreviewBtn.addEventListener(startEvent, () => {
    wasmModule.block_preview_pressed();
  });
  for (const event of ["touchend", "touchcancel", "mouseup", "mouseleave"]) {
    aButton.addEventListener(event, () => {
      wasmModule.a_button_released();
    });
    bButton.addEventListener(event, () => {
      wasmModule.b_button_released();
    });
    // yButton.addEventListener(event, () => {
    //   wasmModule.y_button_released();
    // });
    blockPreviewBtn.addEventListener(event, () => {
      wasmModule.block_preview_released();
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

// Delay mounting joysticks to avoid a bug where the joysticks are
// centered incorrectly on mobile
const JOYSTICK_MOUNT_DELAY = 400;

let joystickMountTimeout: any;
let pitchYawJoystick: nipplejs.JoystickManager | null = null;
let translationJoystick: nipplejs.JoystickManager | null = null;

function mountJoysticks(wasmModule: any) {
  if (!isTouchDevice()) {
    // No joysticks on desktop
    return;
  }

  clearTimeout(joystickMountTimeout);
  if (pitchYawJoystick) pitchYawJoystick.destroy();
  if (translationJoystick) translationJoystick.destroy();

  setTimeout(() => {
    const pitchYawJoystickElem = document.getElementById("pitch-yaw-joystick");
    pitchYawJoystick = nipplejs.create({
      zone: pitchYawJoystickElem,
      color: "transparent",
    });

    let lastPitchYawStartTime = 0;
    let jumpCount = 0;
    const SIMULATE_JUMP_DELAY = 100; // If we tap the joystick, simulate a jump
    pitchYawJoystick.on("move", function (_, data) {
      const now = new Date().getTime();
      if (now - lastPitchYawStartTime < SIMULATE_JUMP_DELAY) return;

      console.log("Pitch/Yaw: ", data.vector);
      wasmModule.pitch_yaw_joystick_moved(data.vector.x, -data.vector.y);
    });
    pitchYawJoystick.on("start", function (_, data) {
      lastPitchYawStartTime = new Date().getTime();
    });
    pitchYawJoystick.on("end", function (_, data) {
      const now = new Date().getTime();
      if (now - lastPitchYawStartTime < SIMULATE_JUMP_DELAY) {
        // Jump if we tap the joystick
        jumpCount += 1;
        wasmModule.y_button_pressed();
        setTimeout(() => wasmModule.y_button_released(), 20);

        if (jumpCount === HIDE_JOYSTICK_HINTS_AFTER_N_JUMPS) hideJoystickHints();
      } else {
        wasmModule.pitch_yaw_joystick_released();
      }
    });
    pitchYawJoystickElem.addEventListener("touchstart", (event) =>
      preventDoubleTapZoom(event)
    );

    const translationJoystickElem = document.getElementById(
      "translation-joystick"
    );
    translationJoystick = nipplejs.create({
      zone: translationJoystickElem,
      color: "transparent",
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
  }, JOYSTICK_MOUNT_DELAY);
}

import("../pkg/index").then((wasmModule) => {
  console.log("WASM Loaded");

  registerDomButtonEventListeners(wasmModule);
  mountJoysticks(wasmModule);

  const controlsInfoPopup = document.getElementById("controls-info-popup");
  document.addEventListener("pointerlockchange", () => {
    if (!document.pointerLockElement) {
      wasmModule.web_pointer_lock_lost();
      if (!isTouchDevice()) controlsInfoPopup.style.display = "block";
    } else {
      if (!isTouchDevice()) controlsInfoPopup.style.display = "none";
    }
  }, false);

  const handleCanvasDidMount = (canvasElem: HTMLCanvasElement) => {
    // Request pointer lock in Safari in JS. Doesn't work from winit Rust in Safari
    if (isSafari) {
      canvasElem.addEventListener("click", async () => {
        if (document.pointerLockElement !== canvasElem) {
          await canvasElem.requestPointerLock();
        }
      });
    }

    const showPortraitOrientationWarning = () => {
      const portraitWarning = document.getElementById("portrait-orientation-warning");
      if (screen.orientation.type.includes("portrait")) {
        portraitWarning.style.display = "flex";
      } else {
        portraitWarning.style.display = "none";
      }
    };
    window.addEventListener("orientationchange", showPortraitOrientationWarning);
    showPortraitOrientationWarning();


    mountJoysticks(wasmModule);
  };

  const wasmContainer = document.getElementById("wasm-container")
  const observerCanvasMounted = (mutationsList: any, observer: any) => {
    for (const mutation of mutationsList) {
      if (mutation.type === 'childList') {
        for (const node of mutation.addedNodes) {
          if (node.nodeName === 'CANVAS' && node.id === 'wasm-canvas') {
            handleCanvasDidMount(node);
            observer.disconnect();
          }
        }
      }
    }
  };
  const observer = new MutationObserver(observerCanvasMounted);
  observer.observe(wasmContainer, { childList: true, subtree: true });

  let resizeTimeout: any;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      const viewportWidth = document.documentElement.clientWidth;
      const viewportHeight = document.documentElement.clientHeight;
      wasmModule.web_window_resized(viewportWidth, viewportHeight);

      // We recreate joysticks, otherwise they start to behave weirdly
      mountJoysticks(wasmModule);
    }, 400);
  });

  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = document.documentElement.clientHeight;

  // async
  wasmModule.run(viewportWidth, viewportHeight);
});