---
title: "Crypto fees are disposals of the fee tokens; every input quantity excludes the fee"
date: 2026-10-08
category: logic-errors
module: core/transactions/convert
problem_type: logic_error
component: fee_handling
severity: high
symptoms:
  - Pool quantities exceed real holdings (live ledger, before the fix - BTC 0.1766 pooled vs 0.0100 held; USDT 1,385 vs 0.06)
  - A fee paid in a third token (e.g. BNB) is never recorded as a disposal of that token
  - Fees on linked transfers are dropped entirely
root_cause: missing_logic
resolution_type: code_fix
related_components:
  - linked-transfers
  - cgt-matching
tags:
  - fees
  - crypto22280
  - linked-transfer
  - quantity-convention
  - akku
---

# Crypto fees are disposals of the fee tokens; every input quantity excludes the fee

## Problem

HMRC (CRYPTO22280) treats tokens spent on a fee as a **disposal of those tokens** at market value, with the fee's value an allowable cost of the transaction it paid for. taxc recorded only the allowable cost, so fee tokens never left their Section 104 pool, a fee in a third token was never a disposal, and fees on linked transfers were dropped because conversion returned before reading them.

## Solution

`EventContext::fee_disposal` emits a disposal of `fee.amount` of `fee.asset`, valued like the fee, on every conversion path (trades, tagged and unlinked deposits/withdrawals, linked transfers, sterling moves), except a GBP or zero fee, or a transaction `--exclude-unlinked` drops. A linked withdrawal that sent more than its deposit received also emits an unclassified **transfer shortfall** disposal of `sent - received`.

## The quantity convention, and how it was settled

The fix is only correct if fee tokens are not *also* inside the traded quantity. The convention is: **every quantity excludes the fee** — `sold`, `bought` and `amount` are the amounts traded or moved; the fee is a separate outflow recorded only in `fee`. A buy of 1 ETH with 0.01 ETH taken from it is `bought: 1` plus `fee: 0.01 ETH`.

This was settled empirically, not assumed. On the live ledger (`akku export-taxc`, 11,631 transactions, 1,204 crypto fees, 2026-10-08):

- With fee disposals, final pools match akku's holdings exactly for ETH (0.00874137), USDC (221,886.73) and USDT (0.0565); without them, taxc over-pooled by up to thousands of units.
- Across all 912 linked transfer pairs the withdrawal and deposit quantities are **equal**, with fees on either leg recorded separately — so the transfer shortfall is `sent - received`, with no fee subtracted. Subtracting the fee assumed the opposite (gross) convention and was the bug a later review caught.

## Prevention

- Before changing quantity semantics, test the convention against a real ledger: compare `taxc pools --json` final balances with the source system's holdings, for both builds.
- Do not "fix" a received-asset fee by skipping its disposal: under this convention the bought quantity does not include the fee, so the disposal is what removes it.
- Tests that encode a convention should say so in their setup (see `crypto_fee_on_linked_transfer_is_a_disposal_of_the_fee_tokens` and `linked_deposit_fee_is_disposed_of_once`).
