declare module 'bs58' {
  export function encode(buffer: Buffer | Uint8Array): string;
  export function decode(string: string): Uint8Array;
}
