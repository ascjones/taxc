use std::collections::{HashMap, HashSet};
use std::error::Error;

use prettytable::Table;
use steel_cent::{currency::Currency, Money};

use crate::{display_amount, Account, Entry, Journal};

struct Balance {
    currency: Currency,
    accounts: HashSet<Account>,
    debits: HashMap<Account, Money>,
    credits: HashMap<Account, Money>,
}
impl Balance {
    fn new(currency: Currency) -> Self {
        Balance {
            currency,
            accounts: HashSet::new(),
            debits: HashMap::new(),
            credits: HashMap::new(),
        }
    }

    fn credit(&mut self, entry: &Entry) {
        self.accounts.insert(entry.account.clone());
        let account_credit_total = self
            .credits
            .entry(entry.account.clone())
            .or_insert(Money::zero(self.currency));
        *account_credit_total = *account_credit_total + entry.amount;
    }

    fn debit(&mut self, entry: &Entry) {
        self.accounts.insert(entry.account.clone());
        let account_debit_total = self
            .debits
            .entry(entry.account.clone())
            .or_insert(Money::zero(self.currency));
        *account_debit_total = *account_debit_total + entry.amount;
    }

    fn total_debits(&self) -> Money {
        self.debits
            .iter()
            .fold(Money::zero(self.currency), |acc, (_, amt)| acc + amt)
    }

    fn total_credits(&self) -> Money {
        self.credits
            .iter()
            .fold(Money::zero(self.currency), |acc, (_, amt)| acc + amt)
    }

    fn balance(&self) -> Money {
        self.total_credits() - self.total_debits()
    }
}

pub fn display_balances(journal: &Journal) -> Result<(), Box<Error>> {
    let mut balances = HashMap::new();
    for tx in journal.transactions() {
        let debit_currency_balance = balances
            .entry(tx.debit.amount.currency)
            .or_insert(Balance::new(tx.debit.amount.currency));
        debit_currency_balance.debit(&tx.debit);
        let credit_currency_balance = balances
            .entry(tx.credit.amount.currency)
            .or_insert(Balance::new(tx.credit.amount.currency));
        credit_currency_balance.credit(&tx.credit);
    }
    let mut table = Table::new();
    table.add_row(row!["Account", "Debit", "Credit", "Balance"]);
    for (currency, balance) in balances.iter() {
        table.add_row(row![currency.code()]);
        let zero = Money::zero(*currency);
        for acct in balance.accounts.iter() {
            let debit_total = balance.debits.get(acct).unwrap_or(&zero);
            let credit_total = balance.credits.get(acct).unwrap_or(&zero);
            let balance = credit_total - debit_total;
            table.add_row(row![
                acct,
                display_amount(debit_total),
                display_amount(credit_total),
                display_amount(&balance),
            ]);
        }
        table.add_row(row![
            "TOTAL",
            display_amount(&balance.total_debits()),
            display_amount(&balance.total_credits()),
            display_amount(&balance.balance()),
        ]);
    }
    table.printstd();
    Ok(())
}
