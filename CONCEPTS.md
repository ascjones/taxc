# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Transactions & Events

### Transaction
A user-supplied input record — a Trade, Deposit, or Withdrawal of an asset on an account at a point in time. Transactions are what users provide; the calculator never taxes them directly.

A Trade generates up to two Taxable Events (a Disposal of the sold asset and an Acquisition of the bought asset); Deposits and Withdrawals generate at most one, depending on their Tag and linkage. A Deposit and Withdrawal pair can be linked to represent an internal transfer, which generates no events.

### Taxable Event
The unit the CGT engine consumes: an Acquisition or Disposal of a quantity of an asset with a GBP value, derived from a Transaction. Each event carries a stable sequential id that survives filtering and ordering.

### Acquisition
A Taxable Event that adds quantity (and allowable cost) to the holder's position in an asset. Income-tagged acquisitions also count toward income totals.

### Disposal
A Taxable Event that reduces the holder's position and triggers a capital gains computation: proceeds minus allowable cost (determined by the Matching Rules) minus fees.

## CGT Matching

### Matching Rules
The UK HMRC-prescribed order for matching a Disposal against Acquisitions to determine its allowable cost: Same-Day first, then Bed & Breakfast, then the Section 104 Pool. A single disposal may be satisfied by a mix of rules.

### Same-Day
Matching rule that pairs a Disposal with Acquisitions of the same asset on the same calendar day, before any other rule applies.

### Bed & Breakfast
Matching rule that pairs a Disposal with Acquisitions of the same asset made within the 30 days *after* the disposal. Exists to neutralise sell-and-rebuy washes around a tax year boundary.
*Avoid:* B&B in prose; the abbreviation is fine in code and badges.

### Section 104 Pool
The per-asset running pool of all unmatched Acquisitions, carrying total quantity and total allowable cost at an averaged basis. Disposals not consumed by Same-Day or Bed & Breakfast draw their cost from this pool.

### No Gain No Loss
A transfer (typically between spouses) that is a Disposal for matching purposes but deemed to realise neither gain nor loss: the recipient inherits the transferor's allowable cost basis rather than market value.

## Classification & Status

### Tag
The classification a user assigns to a Transaction (e.g. Trade, Staking Reward, Gift, Dividend, Interest, No Gain No Loss) that determines how its events are treated — whether they count as income, qualify for CGT, or transfer basis.

### Unclassified
The status of a Transaction (and its derived events) that has no Tag. Unclassified disposals are excluded from headline CGT totals and reported separately as conservative "including unclassified" figures, and each carries a warning so the user knows classification work remains.

### Tax Year
The UK tax year running 6 April to 5 April, displayed as a pair like 2024/25. All CGT and income totals aggregate by Tax Year, not calendar year.
