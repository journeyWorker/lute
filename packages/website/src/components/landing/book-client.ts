/**
 * The landing's one authored interaction.
 *
 * Choosing a "turn to" line in section 1 jumps to that section with a
 * page-turn cut and writes the choice's real outcome onto the adventure sheet
 * (the values `lute trace scene.lute --choose number=<id>` reports). The
 * sheet's seal and note follow the section in view. Reduced motion: the jump
 * and the ticks happen with no transition.
 */

/** Final scene state per choice, from the `lute trace` transcripts. */
const OUTCOMES = {
  blunt: { "scene.affect.marina": "0", "scene.choices.number": "blunt" },
  soft: { "scene.affect.marina": "1", "scene.choices.number": "soft" },
} as const;

type Choice = keyof typeof OUTCOMES;

export function mountBook(book: HTMLElement): void {
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
  const sections = [...book.querySelectorAll<HTMLElement>("[data-section]")];
  const navLinks = [...book.querySelectorAll<HTMLAnchorElement>("[data-nav]")];
  const sheetSection = book.querySelector<HTMLElement>("[data-sheet-section]");
  const seal = book.querySelector<HTMLElement>(".gb-sheet [data-seal]");
  const note = book.querySelector<HTMLElement>(".gb-sheet [data-note]");
  const firstSeal = seal ? [...seal.querySelectorAll<HTMLElement>("[data-seal-line]")].map((l) => l.textContent ?? "") : [];
  const firstNote = note ? [...note.children].map((l) => l.textContent ?? "") : [];

  // --- the sheet follows the section in view -------------------------------
  let current = "1";
  const show = (section: HTMLElement) => {
    const n = section.dataset.section ?? "1";
    if (n === current) return;
    current = n;
    if (sheetSection) sheetSection.textContent = n;
    book.dataset.inView = n;
    navLinks.forEach((a, i) => {
      if (String(i + 1) === n) a.setAttribute("aria-current", "location");
      else a.removeAttribute("aria-current");
    });
    const sealLines: string[] = section.dataset.proofSeal ? JSON.parse(section.dataset.proofSeal) : firstSeal;
    const noteLines: string[] = section.dataset.proofNote ? JSON.parse(section.dataset.proofNote) : firstNote;
    if (seal) {
      seal.querySelectorAll<HTMLElement>("[data-seal-line]").forEach((line, i) => {
        line.textContent = sealLines[i] ?? "";
      });
      seal.setAttribute("aria-label", `${seal.dataset.top}: ${sealLines.join(" ")}, ${seal.dataset.bottom}`);
      restamp(seal);
    }
    if (note) {
      [...note.children].forEach((line, i) => {
        line.textContent = noteLines[i] ?? "";
      });
    }
  };

  const restamp = (el: HTMLElement) => {
    if (reduced.matches) return;
    el.classList.remove("is-stamped");
    void el.offsetWidth;
    el.classList.add("is-stamped");
  };

  // The section in view is the last one whose top has crossed the reading
  // line (45% down the viewport), measured on scroll and once per frame.
  let frame = 0;
  const follow = () => {
    frame = 0;
    const line = window.innerHeight * 0.46;
    const inView = sections.filter((s) => s.getBoundingClientRect().top <= line).at(-1) ?? sections[0];
    if (inView) show(inView);
  };
  window.addEventListener(
    "scroll",
    () => {
      if (!frame) frame = requestAnimationFrame(follow);
    },
    { passive: true },
  );
  follow();

  // --- the signature: turn to a section, tick the sheet --------------------
  const tick = (choice: Choice) => {
    for (const [path, value] of Object.entries(OUTCOMES[choice])) {
      const box = book.querySelector<HTMLElement>(`.gb-sheet [data-state="${path}"]`);
      const valueEl = box?.querySelector<HTMLElement>("[data-value]");
      const wasEl = box?.querySelector<HTMLElement>("[data-was]");
      if (!box || !valueEl || !wasEl) continue;
      const before = valueEl.textContent ?? "";
      if (before === value) continue;
      wasEl.textContent = before;
      box.querySelector("[data-default]")?.remove();
      valueEl.textContent = value;
      if (!reduced.matches) {
        box.classList.remove("is-ticked");
        void box.offsetWidth;
      }
      box.classList.add("is-ticked");
    }
  };

  book.querySelectorAll<HTMLAnchorElement>("a[data-choose]").forEach((link) => {
    link.addEventListener("click", (event) => {
      const target = document.querySelector<HTMLElement>(link.hash);
      const choice = link.dataset.choose as Choice | undefined;
      if (!target || !choice) return;
      event.preventDefault();
      const turn = () => {
        window.scrollTo({ top: target.getBoundingClientRect().top + window.scrollY, behavior: "instant" });
        target.focus({ preventScroll: true });
        history.pushState(null, "", link.hash);
        show(target);
        tick(choice);
      };
      if (reduced.matches || !document.startViewTransition) turn();
      else document.startViewTransition(turn);
    });
  });

  // --- the install control copies its command -------------------------------
  const status = book.querySelector<HTMLElement>(".gb-copied");
  book.querySelectorAll<HTMLButtonElement>("[data-copy]").forEach((button) => {
    button.addEventListener("click", async () => {
      const command = button.dataset.copy ?? "";
      try {
        await navigator.clipboard.writeText(command);
      } catch {
        return;
      }
      button.classList.add("is-copied");
      if (status) status.textContent = book.dataset.copied ?? "";
      window.setTimeout(() => {
        button.classList.remove("is-copied");
        if (status) status.textContent = "";
      }, 1800);
    });
  });
}
