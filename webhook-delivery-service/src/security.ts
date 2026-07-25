import crypto from 'crypto';
import nacl from 'tweetnacl';
import bs58 from 'bs58';

export interface SignatureOutput {
  hmacSignature: string;
  ed25519Signature?: string;
}

export interface SsrfCheckResult {
  valid: boolean;
  reason?: string;
}

/**
 * Validates a destination URL to prevent SSRF (Server-Side Request Forgery) attacks
 */
export function validateUrlForSsrf(urlStr: string): SsrfCheckResult {
  try {
    const url = new URL(urlStr);

    // 1. Enforce HTTP/HTTPS protocols only
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return { valid: false, reason: 'Invalid protocol. Only http: and https: are allowed.' };
    }

    const host = url.hostname.toLowerCase();

    // 2. Prevent empty host
    if (!host) {
      return { valid: false, reason: 'Empty host name.' };
    }

    // 3. Block loopback and standard localhost domains
    if (
      host === 'localhost' ||
      host === '127.0.0.1' ||
      host === '0.0.0.0' ||
      host === '[::1]' ||
      host === '::1'
    ) {
      return { valid: false, reason: 'Loopback and localhost destinations are restricted.' };
    }

    // 4. Block AWS/Metadata IP
    if (host === '169.254.169.254') {
      return { valid: false, reason: 'Metadata services are restricted.' };
    }

    // 5. Block private RFC1918 IP addresses
    // Check if the host matches a private IP address pattern
    const ipv4Pattern = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;
    const ipv4Match = host.match(ipv4Pattern);

    if (ipv4Match) {
      const octet1 = parseInt(ipv4Match[1], 10);
      const octet2 = parseInt(ipv4Match[2], 10);

      // 10.0.0.0/8
      if (octet1 === 10) {
        return { valid: false, reason: 'Private IP space (10.0.0.0/8) is restricted.' };
      }
      // 172.16.0.0/12
      if (octet1 === 172 && octet2 >= 16 && octet2 <= 31) {
        return { valid: false, reason: 'Private IP space (172.16.0.0/12) is restricted.' };
      }
      // 192.168.0.0/16
      if (octet1 === 192 && octet2 === 168) {
        return { valid: false, reason: 'Private IP space (192.168.0.0/16) is restricted.' };
      }
    }

    return { valid: true };
  } catch (error) {
    return { valid: false, reason: 'Invalid URL format.' };
  }
}

/**
 * Generates cryptographic signatures for the request payload
 * Includes HMAC-SHA256 signature and optional Ed25519 signature
 */
export function generateSignatures(
  body: string,
  timestamp: number,
  secret: string,
  privateKeyHex?: string
): SignatureOutput {
  // Construct signature payload to prevent replay and body tampering
  const signaturePayload = `${timestamp}.${body}`;

  // HMAC-SHA256 Signature (Hex-encoded)
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(signaturePayload);
  const hmacSignature = hmac.digest('hex');

  // Ed25519 Asymmetric Signature (optional)
  let ed25519Signature: string | undefined;
  if (privateKeyHex) {
    try {
      // support both raw hex or base58 encoded key
      let privateKeyBytes: Uint8Array;
      if (privateKeyHex.length === 64) {
        // Raw hex representation
        privateKeyBytes = Buffer.from(privateKeyHex, 'hex');
      } else {
        // Base58 encoded (e.g. Stellar secret seed format or custom bs58)
        privateKeyBytes = bs58.decode(privateKeyHex);
      }

      // If the secret key is 32 bytes, we expand it to 64 bytes key pair using tweetnacl
      let secretKey: Uint8Array;
      if (privateKeyBytes.length === 32) {
        const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes);
        secretKey = keyPair.secretKey;
      } else {
        secretKey = privateKeyBytes;
      }

      const messageBytes = Buffer.from(signaturePayload, 'utf-8');
      const signatureBytes = nacl.sign.detached(messageBytes, secretKey);
      ed25519Signature = Buffer.from(signatureBytes).toString('base64');
    } catch (err) {
      // Fallback or ignore invalid keys in signing
      console.error('Ed25519 signing failed:', err);
    }
  }

  return {
    hmacSignature,
    ed25519Signature,
  };
}

/**
 * Verify a webhook payload using HMAC-SHA256
 */
export function verifyHmacSignature(
  body: string,
  timestamp: number,
  signature: string,
  secret: string,
  toleranceSeconds: number = 300 // 5 minutes default
): boolean {
  // 1. Replay attack check (timestamp verification)
  const currentTimestamp = Math.floor(Date.now() / 1000);
  if (Math.abs(currentTimestamp - timestamp) > toleranceSeconds) {
    return false;
  }

  // 2. Recalculate signature
  const expectedPayload = `${timestamp}.${body}`;
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(expectedPayload);
  const expectedSignature = hmac.digest('hex');

  // 3. Timing-safe comparison to prevent timing side-channel attacks
  try {
    return crypto.timingSafeEqual(
      Buffer.from(signature, 'hex'),
      Buffer.from(expectedSignature, 'hex')
    );
  } catch (err) {
    return false;
  }
}

/**
 * Verify a webhook payload using Ed25519 Asymmetric signature
 */
export function verifyEd25519Signature(
  body: string,
  timestamp: number,
  signatureBase64: string,
  publicKeyHexOrBase58: string,
  toleranceSeconds: number = 300
): boolean {
  // 1. Replay attack check
  const currentTimestamp = Math.floor(Date.now() / 1000);
  if (Math.abs(currentTimestamp - timestamp) > toleranceSeconds) {
    return false;
  }

  try {
    let publicKeyBytes: Uint8Array;
    if (publicKeyHexOrBase58.length === 64) {
      publicKeyBytes = Buffer.from(publicKeyHexOrBase58, 'hex');
    } else {
      publicKeyBytes = bs58.decode(publicKeyHexOrBase58);
    }

    const messageBytes = Buffer.from(`${timestamp}.${body}`, 'utf-8');
    const signatureBytes = Buffer.from(signatureBase64, 'base64');

    return nacl.sign.detached.verify(messageBytes, signatureBytes, publicKeyBytes);
  } catch (err) {
    return false;
  }
}
