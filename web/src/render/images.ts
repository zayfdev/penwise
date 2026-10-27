/**
 * Pictures on the page: `DrawList.images` holds asset hashes; bitmaps are fetched once from `/assets/{hash}`
 * and the page is redrawn when they arrive.
 */

/** Loads one asset's bytes. */
export type AssetFetcher = (hash: string) => Promise<Blob>;

/** Bitmap cache with in-flight de-duplication; a failed asset is not retried until {@link clearFailed}. */
export class ImageCache {
  private readonly ready = new Map<string, ImageBitmap>();
  private readonly pending = new Set<string>();
  private readonly failed = new Set<string>();

  constructor(
    private readonly fetchAsset: AssetFetcher,
    private readonly onLoad: () => void,
    private readonly decode: (blob: Blob) => Promise<ImageBitmap> = (b) => createImageBitmap(b),
  ) {}

  /** The bitmap if loaded; otherwise starts loading it and returns `null`. */
  get(hash: string): ImageBitmap | null {
    const bitmap = this.ready.get(hash);
    if (bitmap) return bitmap;
    if (!this.pending.has(hash) && !this.failed.has(hash)) this.load(hash);
    return null;
  }

  /** Forgets failures (after a reconnect). */
  clearFailed(): void {
    this.failed.clear();
  }

  private load(hash: string): void {
    this.pending.add(hash);
    this.fetchAsset(hash)
      .then((blob) => this.decode(blob))
      .then((bitmap) => {
        this.ready.set(hash, bitmap);
        this.onLoad();
      })
      .catch(() => {
        this.failed.add(hash);
      })
      .finally(() => {
        this.pending.delete(hash);
      });
  }
}
