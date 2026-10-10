# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Transactions & Events

### Transaction
A user-supplied input record — a Trade, Deposit, Withdrawal, Demerger, Rights Issue, Small Capital Distribution or Fee on an account at a point in time. Transactions are what users provide; the calculator never taxes them directly.

A Trade generates up to two Taxable Events (a Disposal of the sold asset and an Acquisition of the bought asset); Deposits and Withdrawals generate at most one, depending on their Tag and linkage. A Deposit and Withdrawal pair can be linked to represent an internal transfer, which is not a disposal: it generates events only for a fee paid in crypto (a Fee Disposal) and for any Transfer Shortfall.

### Taxable Event
The unit the CGT engine consumes: an Acquisition, Disposal or Pool Adjustment of a quantity of an asset with a GBP value, derived from a Transaction. Each event carries a stable sequential id that survives filtering and ordering.

### Acquisition
A Taxable Event that adds quantity (and allowable cost) to the holder's position in an asset. Income-tagged acquisitions also count toward income totals.

### Disposal
A Taxable Event that reduces the holder's position and triggers a capital gains computation: proceeds minus allowable cost (determined by the Matching Rules) minus fees.

### Fee Disposal
A Disposal of the tokens spent on a fee (HMRC CRYPTO22280), emitted alongside any Transaction whose fee is paid in a non-GBP asset. The fee's value is also an allowable cost of the transaction it paid for. Quantities on a Transaction exclude the fee, which is recorded only as the fee.

### Fee
A Transaction that pays a fee with nothing else moving. Its fee tokens become a Fee Disposal at market value (HMRC CRYPTO22100, CRYPTO22280); a GBP fee produces no event.

### Transfer Shortfall
The amount a linked Withdrawal sent that its Deposit did not receive. It becomes an Unclassified Disposal, so the missing tokens leave the pool and are flagged for review.

## CGT Matching

### Matching Rules
The UK HMRC-prescribed order for matching a Disposal against Acquisitions to determine its allowable cost: Same-Day first, then Bed & Breakfast, then the Section 104 Pool. A single disposal may be satisfied by a mix of rules.

### Same-Day
Matching rule that pairs a Disposal with Acquisitions of the same asset on the same UK calendar day (Europe/London time), before any other rule applies.

### Bed & Breakfast
Matching rule that pairs a Disposal with Acquisitions of the same asset made within the 30 days *after* the disposal. Exists to neutralise sell-and-rebuy washes around a tax year boundary.
*Avoid:* B&B in prose; the abbreviation is fine in code and badges.

### Section 104 Pool
The per-asset running pool of all unmatched Acquisitions, carrying total quantity and total allowable cost at an averaged basis. Disposals not consumed by Same-Day or Bed & Breakfast draw their cost from this pool.

### Pool Adjustment
A Taxable Event that changes a Section 104 Pool directly, without a disposal: a Demerger, Rights Issue or Small Capital Distribution. It is never matched by the Matching Rules and never counts as an acquisition, disposal or income. On a UK date, adjustments apply before that day's disposals.

### Share Reorganisation
HMRC's term (TCGA 1992 s.126–s.131) for a change to a company's share capital in which the new holding stands in the shoes of the original shares: no disposal, and the original cost carries over. taxc models three kinds as Pool Adjustments; conversions (s.135) and unit changes at constant cost are out of scope.

### Demerger
A Share Reorganisation in which a company passes shares in another company to its shareholders, treated as an exempt distribution (s.192) or a scheme of reconstruction (s.136). A stated fraction of the original pool's cost moves to the new holding, apportioned by market value: on the first dealing day if the shares are quoted (s.130), otherwise at the first disposal (s.129). HMRC CG45620, CG51702, CG51890, CG52742. A demerger taxed as a dividend in specie is income, not a Demerger.

### Rights Issue
A Share Reorganisation in which the holder takes up their pro-rata entitlement to new shares in the same company (s.126–s.128; HMRC CG51746, CG51590). The shares and their consideration join the existing pool. Purchased rights, excess applications and rights in another company (CG52065) are ordinary acquisitions.

### Small Capital Distribution
A capital distribution small enough to reduce the holding's allowable cost instead of being a part disposal (s.122(2); HMRC CG57835), including cash for fractional entitlements (s.128(3); CG57855). HMRC's practice treats 5% or less of the holding's value, or £3,000 or less, as small. When it exceeds the pool's cost, the excess is a gain under a s.122(4) election (CG57847), recorded on the distribution with a warning.

### No Gain No Loss
A transfer (typically between spouses) that is a Disposal for matching purposes but deemed to realise neither gain nor loss: the recipient inherits the transferor's allowable cost basis rather than market value.

## Classification & Status

### Tag
The classification a user assigns to a Transaction (e.g. Trade, Staking Reward, Gift, Dividend, Interest, Cashback, No Gain No Loss) that determines how its events are treated — whether they count as income, qualify for CGT, or transfer basis. Cashback is an ordinary acquisition at market value but is not income: HMRC treats cashback on personal spending as tax-free (Statement of Practice 4/97).

### Unclassified
The status of a Transaction (and its derived events) that has no Tag. Unclassified disposals are excluded from headline CGT totals and reported separately as conservative "including unclassified" figures, and each carries a warning so the user knows classification work remains.

### Tax Year
The UK tax year running 6 April to 5 April, displayed as a pair like 2024/25. All CGT and income totals aggregate by Tax Year, not calendar year. An event's tax year comes from its UK calendar date, whatever offset the input was written in: 23:30 UTC on 5 April in summer is 6 April in the UK.
