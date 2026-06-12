# 🌍 IoT-Billing-Service Multi-Language Error Mapping

This document provides a mapping of on-chain error codes to human-readable descriptions in multiple languages. This ensures accessibility for users in rural areas and non-English speaking regions (Issue #122).

## Error Code Reference

| Code | ID | Description | Yoruba | Hausa | Igbo | Spanish | French |
|------|----|-------------|--------|-------|------|---------|--------|
| 1 | `MeterNotFound` | Meter not registered. | A kò rí mita yìí. | Ba a sami mita ba. | Ahụghị mita a. | Medidor no encontrado. | Compteur non trouvé. |
| 5 | `InvalidTokenAmount` | Invalid token amount. | Iye owó kò tọ́. | Adadin kuɗi ba daidai ba. | Ego ezughị oke. | Cantidad de tokens inválida. | Montant de jetons invalide. |
| 11 | `TimestampTooOld` | Transaction expired. | Àkókò ti kọjá. | Lokaci ya ƙare. | Oge agwụla. | Transacción expirada. | Transaction expirée. |
| 15 | `MeterNotPaired` | Device not paired. | Ẹ̀rọ kò tíì so pọ̀. | Ba a haɗa na'ura ba. | Ejikọtaghị mita. | Dispositivo no vinculado. | Appareil non appairé. |
| 16 | `MeterPaused` | Meter is paused. | Mita ti dádúró. | An dakatar da mita. | Akwụsịrị mita a. | Medidor pausado. | Compteur en pause. |
| 19 | `AccountAlreadyClosed` | Account is closed. | Àkàǹtì ti tì. | An rufe asusu. | Emechiela akaụntụ a. | Cuenta ya cerrada. | Compte déjà fermé. |
| 20 | `InsufficientBalance` | Low balance. | Owó kò tó. | Kuɗi ba su isa ba. | Ego ezughị. | Saldo insuficiente. | Solde insuffisant. |
| 22 | `InDispute` | Service in dispute. | Àríyànjiyàn wà. | Akwai jayayya. | E nwere esemokwu. | Servicio en disputa. | Service en litige. |
| 44 | `ProviderNotVerified` | Provider not verified. | Olùpèsè kò fẹsẹ̀ múlẹ̀. | Ba a tabbatar da mai samarwa ba. | Akwadoghị onye na-enye ọrụ. | Proveedor no verificado. | Fournisseur non vérifié. |
| 49 | `InsufficientXlmReserve` | Gas reserve low. | Owó gas kò tó. | Gas ya yi ƙasa. | Ego gas dị ala. | Reserva de gas insuficiente. | Réserve de gas insuffisante. |

## Backend Integration

The backend service should intercept contract reverts, extract the `u32` error code, and look up the corresponding translation based on the user's localized settings.

### Example Mapping (JSON)
```json
{
  "20": {
    "en": "Insufficient balance to continue service.",
    "yo": "Owó kò tó láti tẹ̀síwájú.",
    "ha": "Kuɗi ba su isa su ci gaba da sabis ba.",
    "ig": "Ego ezughị iji gaa n'ihu.",
    "es": "Saldo insuficiente para continuar el servicio.",
    "fr": "Solde insuffisant pour continuer le service."
  }
}
```

**Last Updated**: March 26, 2026