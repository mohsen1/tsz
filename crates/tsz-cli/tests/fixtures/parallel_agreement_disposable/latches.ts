export interface ScopedSealing {
  [Symbol.asyncDispose](): Promise<void>;
}

export class VaultLatch implements ScopedSealing {
  readonly #notes: string[] = [];

  public constructor(
    private readonly meta: { readonly namespace: string; readonly tags: readonly string[] },
    private readonly onSeal: () => Promise<undefined>,
  ) {}

  public note(entry: string): void {
    this.#notes.push(`${this.meta.namespace}:${entry}`);
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.onSeal();
  }
}
