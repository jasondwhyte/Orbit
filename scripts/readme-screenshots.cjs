// Takes the README screenshots from the browser demo (npm run dev).
// Usage: node take.cjs <out-dir> [scale] [bit-height] [live-height]
const { chromium } = require("playwright");
const out = process.argv[2];
const scale = Number(process.argv[3] ?? 1.5);
const BIT_HEIGHT = Number(process.argv[4] ?? 940);
const LIVE_HEIGHT = Number(process.argv[5] ?? 1220);

// Hide the "Interactive demo" banner, which only the demo shows.
async function hideBanner(page) {
  await page.evaluate(() => {
    const all = [...document.querySelectorAll("*")];
    let el = all
      .filter(
        (e) =>
          e.innerText?.includes("Interactive demo") &&
          e.innerText?.includes("Connect your workspace"),
      )
      .sort((a, b) => a.innerText.length - b.innerText.length)[0];
    if (!el) return;
    while (
      el.parentElement &&
      el.parentElement.innerText.trim() === el.innerText.trim()
    )
      el = el.parentElement;
    el.style.display = "none";
  });
  await page.waitForTimeout(300);
}

async function nav(page, name) {
  await page
    .getByRole("navigation")
    .getByText(name, { exact: true })
    .first()
    .click();
  await page.waitForTimeout(1200);
}

// Scroll the nearest scrolling box so `text` sits `offset` px below its top.
async function scrollTo(page, text, offset = 0) {
  await page.evaluate(
    ([text, offset]) => {
      const el = [
        ...document.querySelectorAll("h1,h2,h3,h4,p,span,div"),
      ].filter(
        (e) =>
          e.childElementCount === 0 && e.textContent.trim().startsWith(text),
      )[0];
      if (!el) throw Error(`no ${text}`);
      let box = el.parentElement;
      while (box) {
        const s = getComputedStyle(box);
        if (
          (s.overflowY === "auto" || s.overflowY === "scroll") &&
          box.scrollHeight > box.clientHeight
        )
          break;
        box = box.parentElement;
      }
      if (!box) throw Error("no scroll box");
      const top =
        el.getBoundingClientRect().top - box.getBoundingClientRect().top;
      box.scrollTop += top - offset;
    },
    [text, offset],
  );
  await page.waitForTimeout(500);
}

// Hide the "Scroll to bottom" button a scrolled chat shows.
async function hideScrollButton(page) {
  await page.evaluate(() => {
    for (let b of document.querySelectorAll(
      'button[aria-label="Scroll to bottom"]',
    )) {
      while (b.parentElement && b.parentElement.childElementCount === 1)
        b = b.parentElement;
      b.style.visibility = "hidden";
    }
  });
}

async function shot(page, name) {
  await hideBanner(page);
  await hideScrollButton(page);
  await page.mouse.move(1430, 890);
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${out}/${name}.png` });
  console.log("took", name);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: scale,
  });
  await page.goto("http://127.0.0.1:1420");
  await page.waitForTimeout(2000);

  // Review chat on the first pull request.
  await page.getByRole("tab", { name: /Review chat/ }).click();
  await page.waitForTimeout(1200);
  await scrollTo(page, "F2 · Validate", 12);
  await shot(page, "chat");

  // A loop, round by round.
  await nav(page, "Review loops");
  await page.getByText("Blocking findings 3 → 2 → 2 → 1").first().click();
  await page.waitForTimeout(1200);
  await shot(page, "loops");

  await nav(page, "Operations");
  await scrollTo(page, "Open problems", 20);
  await shot(page, "operations");

  await nav(page, "Findings");
  await shot(page, "findings");

  await nav(page, "Decisions");
  await shot(page, "decisions");

  // Bit, with the morning brief at the top of its notes.
  await page.setViewportSize({ width: 1440, height: BIT_HEIGHT });
  await page.getByText("Heads up!", { exact: false }).first().click();
  await page.waitForTimeout(1200);
  await scrollTo(page, "Here's your morning brief", 26);
  await page.evaluate(() => {
    let b = document.querySelector('button[aria-label="Scroll to bottom"]');
    while (b && b.parentElement && b.parentElement.childElementCount === 1)
      b = b.parentElement;
    if (b) b.style.visibility = "hidden";
  });
  await shot(page, "bit");
  await page.setViewportSize({ width: 1440, height: 900 });

  // Live, with a review running.
  await nav(page, "All pull requests");
  await page.getByRole("button", { name: "Review again" }).first().click();
  await page.waitForTimeout(1200);
  await page.setViewportSize({ width: 1440, height: LIVE_HEIGHT });
  await nav(page, "Live");
  await page.getByText("Per day", { exact: true }).first().click();
  await page.waitForTimeout(800);
  await shot(page, "live");
  await page.setViewportSize({ width: 1440, height: 900 });

  // Bit's looks, from Settings > Buddy.
  await nav(page, "Settings");
  await page.getByRole("tab", { name: "Buddy" }).click();
  await page.waitForTimeout(1200);
  await scrollTo(page, "Look", 40);
  await hideBanner(page);
  const grid = await page.evaluate(() => {
    const label = [...document.querySelectorAll("*")].find(
      (e) => e.childElementCount === 0 && e.textContent.trim() === "Look",
    );
    const first = [...document.querySelectorAll("*")].find(
      (e) => e.childElementCount === 0 && e.textContent.trim() === "Classic",
    );
    const last = [...document.querySelectorAll("*")].find(
      (e) => e.childElementCount === 0 && e.textContent.trim() === "Fold",
    );
    const nova = [...document.querySelectorAll("*")].find(
      (e) => e.childElementCount === 0 && e.textContent.trim() === "Nova",
    );
    // The tile around each label.
    const tile = (e) => {
      while (e.parentElement && e.getBoundingClientRect().height < 60)
        e = e.parentElement;
      return e.getBoundingClientRect();
    };
    const a = tile(first),
      z = tile(last),
      n = tile(nova);
    return { x: a.left, y: a.top, w: n.right - a.left, h: z.bottom - a.top };
  });
  const pad = 12;
  await page.mouse.move(1430, 10);
  await page.waitForTimeout(300);
  await page.screenshot({
    path: `${out}/bit-looks.png`,
    clip: {
      x: grid.x - pad,
      y: grid.y - pad,
      width: grid.w + 2 * pad,
      height: grid.h + 2 * pad,
    },
  });
  console.log("took bit-looks", JSON.stringify(grid));

  // The phone, on a pull request whose loop needs you.
  const phone = await browser.newPage({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: scale === 1 ? 1 : 2,
    isMobile: true,
    hasTouch: true,
  });
  await phone.goto("http://127.0.0.1:1420");
  await phone.waitForTimeout(2000);
  await phone
    .getByText("Keep review threads in sync with new commits")
    .first()
    .click();
  await phone.waitForTimeout(1200);
  await hideBanner(phone);
  await phone.screenshot({ path: `${out}/phone.png` });
  console.log("took phone");

  await browser.close();
})();
