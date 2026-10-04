// The diagram viewer: each inlined view sits in a frame at a scale a
// reader can read, with Fit, 1:1, Wide and Fullscreen controls. Without
// this script a view is scaled to the text column (the stylesheet's
// rule) and the controls stay hidden, so a page with no JavaScript shows
// a diagram and no dead buttons.
//
// Scale: a view is shown at the scale that fits the frame's width, held
// between MIN and 1, so a wide view scrolls inside its frame instead of
// shrinking its text to nothing, and a small one is not blown up. Fit
// shows the whole view, width and height, at whatever scale that takes;
// 1:1 shows it at the size LikeC4 laid it out. Wide lets every figure on
// the site take the window's width, remembered like the color scheme;
// Fullscreen takes one figure to the screen.
(function () {
  var WIDE_KEY = "asbuilt-docs-wide";
  var MIN = 0.7;
  var GUTTER = 16;
  var FRAME_SHARE = 0.75; // of the window's height; the stylesheet's 75vh
  var root = document.documentElement;
  var wide = false;
  try {
    wide = window.localStorage.getItem(WIDE_KEY) === "1";
  } catch (e) {}
  var figures = [];

  function viewBoxSize(svg) {
    var parts = (svg.getAttribute("viewBox") || "").trim().split(/[\s,]+/);
    var width = parseFloat(parts[2]);
    var height = parseFloat(parts[3]);
    return width > 0 && height > 0 ? { width: width, height: height } : null;
  }

  function layoutAll() {
    figures.forEach(function (f) {
      f.layout();
    });
  }

  function setup(figure) {
    var svg = figure.querySelector("svg.c4");
    var frame = figure.querySelector(".viewer-frame");
    var bar = figure.querySelector(".viewer-bar");
    if (!svg || !frame || !bar) return;
    var natural = viewBoxSize(svg);
    if (!natural) return;
    var mode = "auto";
    var readout = bar.querySelector(".viewer-scale");
    var full = bar.querySelector("[data-viewer-full]");

    function fullscreen() {
      return document.fullscreenElement === figure;
    }
    function widthFit() {
      return frame.clientWidth / natural.width;
    }
    // The height the frame may take: the stylesheet's share of the
    // window, or in fullscreen the screen below the controls.
    function heightBudget() {
      if (fullscreen()) {
        return window.innerHeight - frame.getBoundingClientRect().top - GUTTER;
      }
      return FRAME_SHARE * window.innerHeight;
    }
    function scale() {
      if (mode === "one") return 1;
      if (mode === "fit") return Math.min(widthFit(), heightBudget() / natural.height);
      return Math.min(1, Math.max(MIN, widthFit()));
    }
    function layout() {
      if (wide && !fullscreen()) {
        // The window's width, measured from the document (which leaves
        // out the scrollbar), from wherever the text column put us; the
        // root's own left accounts for any horizontal scroll.
        figure.style.marginLeft = "0";
        var left = figure.getBoundingClientRect().left - root.getBoundingClientRect().left;
        figure.style.marginLeft = GUTTER - left + "px";
        figure.style.width = root.clientWidth - 2 * GUTTER + "px";
      } else {
        figure.style.marginLeft = "";
        figure.style.width = "";
      }
      var s = scale();
      svg.style.width = Math.round(natural.width * s) + "px";
      if (readout) readout.textContent = Math.round(s * 100) + "%";
      bar.querySelectorAll("[data-viewer-mode]").forEach(function (button) {
        var pressed = button.getAttribute("data-viewer-mode") === mode;
        button.setAttribute("aria-pressed", String(pressed));
      });
      var wideButton = bar.querySelector("[data-viewer-wide]");
      if (wideButton) wideButton.setAttribute("aria-pressed", String(wide));
    }

    bar.querySelectorAll("[data-viewer-mode]").forEach(function (button) {
      button.addEventListener("click", function () {
        var next = button.getAttribute("data-viewer-mode");
        mode = mode === next ? "auto" : next;
        layout();
      });
    });
    var wideButton = bar.querySelector("[data-viewer-wide]");
    if (wideButton) {
      wideButton.addEventListener("click", function () {
        wide = !wide;
        try {
          window.localStorage.setItem(WIDE_KEY, wide ? "1" : "0");
        } catch (e) {}
        layoutAll();
      });
    }
    if (full) {
      if (figure.requestFullscreen) {
        full.addEventListener("click", function () {
          if (fullscreen()) document.exitFullscreen();
          else figure.requestFullscreen();
        });
      } else {
        full.hidden = true;
      }
    }
    document.addEventListener("fullscreenchange", layout);
    // The frame's width changes with the column; the window's with Wide
    // (which sizes the figure itself, so the frame alone would not tell).
    if (window.ResizeObserver) new ResizeObserver(layout).observe(frame);

    figure.setAttribute("data-viewer-active", "");
    bar.hidden = false;
    layout();
    figures.push({ layout: layout });
  }

  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll("figure[data-viewer]").forEach(setup);
    window.addEventListener("resize", layoutAll);
  });
})();
