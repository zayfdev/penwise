/**
 * Tiny DOM builders. Server data (titles, snippets, recognized text) only ever reaches the page through
 * `textContent` / attributes set here, never `innerHTML`.
 */

type Child = Node | string;

/** `<tag class=…>` with text/element children; attribute values are set as strings. */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: { class?: string; text?: string; attrs?: Record<string, string> } = {},
  children: readonly Child[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (props.class !== undefined) node.className = props.class;
  if (props.text !== undefined) node.textContent = props.text;
  for (const [name, value] of Object.entries(props.attrs ?? {})) node.setAttribute(name, value);
  for (const child of children) node.append(child);
  return node;
}

/** Replaces the children of `parent`. */
export function setChildren(parent: Element, children: readonly Node[]): void {
  parent.replaceChildren(...children);
}

/** `getElementById` that fails loudly instead of returning `null`. */
export function byId<T extends HTMLElement>(id: string, type: new () => T): T {
  const node = document.getElementById(id);
  if (!(node instanceof type)) throw new Error(`#${id} is missing or not a ${type.name}`);
  return node;
}
