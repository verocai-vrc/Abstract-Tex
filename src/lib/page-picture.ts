// When the picture of a document's first page is drawn for Home (S17.15): a while after a build that
// worked, and not while anybody is typing. Drawing one takes some milliseconds of the window's own time,
// and a picture nobody is looking at must never be what a key waits for.

/** How long it is quiet first. */
const QUIET_MS = 5000;

export class PagePictures {
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly draw: () => Promise<void>,
    /** Whether what is on show is the whole document as it is now. Asked when it is time to draw. */
    private readonly canDraw: () => boolean,
  ) {}

  buildFinished(): void {
    this.wait();
  }

  typed(): void {
    if (this.timer !== null) this.wait();
  }

  cancel(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private wait(): void {
    this.cancel();
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.canDraw()) this.draw().catch(() => {}); // Home shows an empty page instead
    }, QUIET_MS);
  }
}
