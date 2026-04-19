/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import nacl from "tweetnacl";

// Citadel Vault Service
// This service simulates the integration of Rust crates like ed25519-dalek, ecdsa, and dsa.
// In the native Sextant browser, these are handled by the Rust core for maximum security.

export interface VaultKey {
  id: string;
  type: "Ed25519" | "ECDSA-P256" | "DSA";
  publicKey: string;
  did: string;
  createdAt: string;
}

export class CitadelVault {
  private isLocked = true;
  private keys: VaultKey[] = [];
  private hardwareAnchorActive = false;

  // Helper to convert Uint8Array to Hex
  private toHex(arr: Uint8Array): string {
    return Array.from(arr).map(b => b.toString(16).padStart(2, '0')).join('');
  }

  // Simulating DID generation (did:key format)
  private generateDID(publicKey: string, type: VaultKey["type"]): string {
    const prefix = type === "Ed25519" ? "z6M" : "zDna";
    return `did:key:${prefix}${publicKey.substring(0, 32)}`;
  }

  async unlock(passphrase: string): Promise<boolean> {
    if (passphrase === "commander") {
      this.isLocked = false;
      this.hardwareAnchorActive = true; // Simulating TPM 2.0 anchoring
      
      if (this.keys.length === 0) {
        await this.generateKey("Ed25519");
        await this.generateKey("ECDSA-P256");
      }
      return true;
    }
    return false;
  }

  lock() {
    this.isLocked = true;
    this.hardwareAnchorActive = false;
  }

  getIsLocked() {
    return this.isLocked;
  }

  getHardwareAnchorStatus() {
    return this.hardwareAnchorActive ? "TPM 2.0 Anchored" : "Unanchored";
  }

  getKeys() {
    return this.isLocked ? [] : this.keys;
  }

  // Simulating the "Captain's Key" (FIDO2/YubiKey) physical consent
  async requestPhysicalConsent(): Promise<boolean> {
    if (this.isLocked) return false;
    // In a real app, this would call the WebAuthn API or a native bridge
    return new Promise((resolve) => {
      // We'll trigger a UI event that App.tsx will listen for
      const event = new CustomEvent("sextant:physical-consent-request", {
        detail: { resolve }
      });
      window.dispatchEvent(event);
    });
  }

  async generateKey(type: VaultKey["type"]): Promise<VaultKey> {
    if (this.isLocked) throw new Error("Vault is locked");

    let publicKey = "";
    const id = Math.random().toString(36).substr(2, 9);

    if (type === "Ed25519") {
      const keyPair = nacl.sign.keyPair();
      publicKey = this.toHex(keyPair.publicKey);
    } else if (type === "ECDSA-P256") {
      const keyPair = await window.crypto.subtle.generateKey(
        { name: "ECDSA", namedCurve: "P-256" },
        true,
        ["sign", "verify"]
      );
      const exported = await window.crypto.subtle.exportKey("spki", keyPair.publicKey);
      publicKey = this.toHex(new Uint8Array(exported)).substring(0, 64);
    } else {
      publicKey = "DSA-PROTOTYPE-KEY-" + id;
    }

    const newKey: VaultKey = {
      id,
      type,
      publicKey,
      did: this.generateDID(publicKey, type),
      createdAt: new Date().toISOString()
    };

    this.keys.push(newKey);
    return newKey;
  }

  async sign(keyId: string, message: string, requireConsent = true): Promise<string> {
    if (this.isLocked) throw new Error("Vault is locked");
    
    if (requireConsent) {
      const consented = await this.requestPhysicalConsent();
      if (!consented) throw new Error("Physical consent denied");
    }

    const key = this.keys.find(k => k.id === keyId);
    if (!key) throw new Error("Key not found");

    const msgBytes = new TextEncoder().encode(message);
    const hash = await window.crypto.subtle.digest("SHA-256", msgBytes);
    return this.toHex(new Uint8Array(hash));
  }
}

export const citadelVault = new CitadelVault();
