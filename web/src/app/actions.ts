/** User actions that talk HTTP: create pages/notebooks, trade a pairing code for the token. */

import { type Api, exchangePairingCode, type PageTemplate } from "../net/api";
import { TOKEN_KEY } from "../net/credentials";

/** What the actions report back. */
export interface ActionEvents {
  /** A page was created (it is shown as soon as the catalog has it). */
  created(pageId: string): void;
  /** A pairing code was accepted; the token is stored. */
  paired(token: string): void;
  problem(message: string): void;
}

/** HTTP-backed user actions. */
export class Actions {
  constructor(
    private readonly api: Pick<Api, "createPage" | "createNotebook" | "sendRound">,
    private readonly events: ActionEvents,
  ) {}

  async createPage(template: PageTemplate, notebook: string | null, title: string): Promise<void> {
    try {
      const id = await this.api.createPage(template, notebook, title);
      if (id !== "") this.events.created(id);
    } catch (error) {
      this.events.problem(`could not create the page: ${String(error)}`);
    }
  }

  async createNotebook(title: string): Promise<void> {
    try {
      await this.api.createNotebook(title);
    } catch (error) {
      this.events.problem(`could not create the notebook: ${String(error)}`);
    }
  }

  /** "Send": closes the project's open round; the words to show next to the button. */
  async sendRound(project: string): Promise<string> {
    try {
      return (await this.api.sendRound(project)) ? "Sent" : "No new marks";
    } catch (error) {
      this.events.problem(`round not sent: ${String(error)}`);
      return "Not sent";
    }
  }

  /** Rejects when the server refuses the code. */
  async pair(code: string): Promise<void> {
    const token = await exchangePairingCode(code);
    localStorage.setItem(TOKEN_KEY, token);
    this.events.paired(token);
  }
}
