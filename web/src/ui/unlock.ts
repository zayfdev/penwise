/** Shown when the browser has no (valid) token: enter the code the server/tablet displays. */

/** Overlay with a code field. */
export class Unlock {
  private readonly input: HTMLInputElement;
  private readonly message: HTMLElement;

  constructor(
    private readonly root: HTMLElement,
    submit: (code: string) => Promise<void>,
  ) {
    this.input = root.querySelector<HTMLInputElement>("input") ?? document.createElement("input");
    this.message = root.querySelector<HTMLElement>("[data-message]") ?? root;
    root.querySelector<HTMLFormElement>("form")?.addEventListener("submit", (event) => {
      event.preventDefault();
      const code = this.input.value.trim();
      if (code === "") return;
      submit(code).catch((error: unknown) => {
        this.message.textContent = `That code was not accepted (${String(error)})`;
      });
    });
  }

  show(reason: string): void {
    this.message.textContent = reason;
    this.root.hidden = false;
    this.input.focus();
  }

  hide(): void {
    this.root.hidden = true;
  }
}
