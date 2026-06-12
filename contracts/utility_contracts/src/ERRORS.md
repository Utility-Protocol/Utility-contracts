# IoT-Billing-Service Contract - Error Codes

This document provides a mapping of on-chain error codes to user-friendly explanations and suggested actions. When a transaction fails, the frontend can use this guide to display a helpful message instead of a raw error.

| Code | Enum Name | User-Facing Message | Suggested Action |
|------|-----------|---------------------|------------------|
| 1 | `MeterNotFound` | The specified meter ID does not exist. | Please double-check the meter ID you entered. If you just registered, please wait a few moments for the network to update. |
| 2 | `OracleNotSet` | The price oracle has not been configured by the admin. | This is a contract configuration issue. Please contact the service provider. |
| 5 | `InvalidTokenAmount` | The amount for the transaction is invalid (e.g., zero or negative). | Please enter a positive amount for your top-up or withdrawal. |
| 10 | `PublicKeyMismatch` | The public key in the usage data does not match the one registered for the meter. | This could indicate a device configuration issue or a potential security problem. Please contact your utility provider. |
| 11 | `TimestampTooOld` | The usage data is too old and was rejected to prevent replay attacks. | Ensure your metering device's clock is synchronized. The issue should resolve itself on the next reading. |
| 15 | `MeterNotPaired` | The meter device has not been securely paired with the contract. | Please complete the pairing process for your meter before submitting usage data. |
| 19 | `AccountAlreadyClosed` | This meter account has already been closed. | You cannot perform actions on a closed account. Please register a new meter if you wish to continue service. |
| 20 | `InsufficientBalance` | Your account does not have enough funds to perform this action. | Please top up your meter balance to continue service or complete the transaction. |
| 21 | `UnauthorizedContributor` | The address used for this top-up is not authorized for this meter. | Only the meter owner or an authorized contributor (e.g., a roommate) can top up this meter. |
| 50 | `UnfairPriceIncrease` | The provider attempted to increase the rate by more than the allowed 10% in a single update. | The transaction was blocked to protect you from a sudden price spike. No action is needed on your part. |
| 51 | `BillingGroupNotFound` | The specified billing group does not exist. | Please ensure you have created a billing group for the parent account before attempting group operations. |