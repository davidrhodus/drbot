use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::account_info::{next_account_info, AccountInfo};
use solana_program::clock::Clock;
use solana_program::entrypoint;
use solana_program::entrypoint::ProgramResult;
use solana_program::msg;
use solana_program::program::{invoke, invoke_signed};
use solana_program::program_error::ProgramError;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::system_instruction;
use solana_program::sysvar::Sysvar;

pub const ESCROW_SEED_PREFIX: &[u8] = b"drbot_otc_escrow";

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum LegKind {
    NativeSol = 0,
    SplToken = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Leg {
    pub kind: LegKind,
    pub mint: Pubkey,
    pub amount: u64,
}

impl Leg {
    pub fn native_sol(amount: u64) -> Self {
        Self {
            kind: LegKind::NativeSol,
            mint: Pubkey::default(),
            amount,
        }
    }

    pub fn spl_token(mint: Pubkey, amount: u64) -> Self {
        Self {
            kind: LegKind::SplToken,
            mint,
            amount,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum EscrowParty {
    PartyA = 0,
    PartyB = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EscrowTerms {
    pub negotiation_id: [u8; 16],
    pub party_a: Pubkey,
    pub party_b: Pubkey,
    pub a_owes: Leg,
    pub b_owes: Leg,
    pub expiry_unix_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EscrowState {
    pub version: u8,
    pub bump: u8,
    pub negotiation_id: [u8; 16],
    pub party_a: Pubkey,
    pub party_b: Pubkey,
    pub rent_refund: Pubkey,
    pub a_owes: Leg,
    pub b_owes: Leg,
    pub a_funded: bool,
    pub b_funded: bool,
    pub expiry_unix_ts: i64,
}

impl EscrowState {
    pub const VERSION: u8 = 1;

    pub const LEN: usize = 1  // version
        + 1                  // bump
        + 16                 // negotiation_id
        + 32                 // party_a
        + 32                 // party_b
        + 32                 // rent_refund
        + (1 + 32 + 8)       // a_owes (kind + mint + amount)
        + (1 + 32 + 8)       // b_owes
        + 1                  // a_funded
        + 1                  // b_funded
        + 8;                 // expiry_unix_ts (i64)
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum EscrowInstruction {
    /// Create an escrow PDA and (if needed) token vault ATAs.
    ///
    /// Idempotent: if the escrow already exists, verifies terms match and returns Ok.
    CreateEscrow { terms: EscrowTerms },
    /// Fund a party's owed leg. Auto-settles (and closes) if this makes escrow fully funded.
    Fund { party: EscrowParty },
    /// Cancel/expire and refund any funded leg(s), then close.
    Cancel,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowError {
    InvalidInstruction = 1,
    InvalidPda = 2,
    InvalidParty = 3,
    InvalidPayer = 4,
    InvalidTokenProgram = 5,
    TermsMismatch = 6,
    EscrowExpired = 7,
    AlreadyFunded = 8,
    InvalidTokenAccount = 9,
    MissingAccounts = 10,
    NotCancelable = 11,
}

impl From<EscrowError> for ProgramError {
    fn from(value: EscrowError) -> Self {
        ProgramError::Custom(value as u32)
    }
}

pub fn derive_escrow_pda(
    program_id: &Pubkey,
    negotiation_id: &[u8; 16],
    party_a: &Pubkey,
    party_b: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            ESCROW_SEED_PREFIX,
            negotiation_id,
            party_a.as_ref(),
            party_b.as_ref(),
        ],
        program_id,
    )
}

entrypoint!(process_instruction);

pub fn process_instruction(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let ix = EscrowInstruction::try_from_slice(data).map_err(|_| EscrowError::InvalidInstruction)?;
    match ix {
        EscrowInstruction::CreateEscrow { terms } => create_escrow(program_id, accounts, terms),
        EscrowInstruction::Fund { party } => fund(program_id, accounts, party),
        EscrowInstruction::Cancel => cancel(program_id, accounts),
    }
}

fn create_escrow(program_id: &Pubkey, accounts: &[AccountInfo], terms: EscrowTerms) -> ProgramResult {
    let mut ai = accounts.iter();
    let payer = next_account_info(&mut ai)?;
    let escrow = next_account_info(&mut ai)?;
    let party_a = next_account_info(&mut ai)?;
    let party_b = next_account_info(&mut ai)?;
    let system_program = next_account_info(&mut ai)?;
    let token_program = next_account_info(&mut ai)?;
    let ata_program = next_account_info(&mut ai)?;
    let rent_sysvar = next_account_info(&mut ai)?;

    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if payer.key != party_a.key && payer.key != party_b.key {
        return Err(EscrowError::InvalidPayer.into());
    }
    if token_program.key != &spl_token::id() {
        return Err(EscrowError::InvalidTokenProgram.into());
    }

    let (expected_escrow, bump) =
        derive_escrow_pda(program_id, &terms.negotiation_id, &terms.party_a, &terms.party_b);
    if &expected_escrow != escrow.key {
        return Err(EscrowError::InvalidPda.into());
    }

    // If already initialized, verify terms match and return Ok.
    if escrow.owner == program_id && !escrow.data_is_empty() {
        let existing =
            EscrowState::try_from_slice(&escrow.data.borrow()).map_err(|_| EscrowError::TermsMismatch)?;
        let desired = EscrowState {
            version: EscrowState::VERSION,
            bump: existing.bump,
            negotiation_id: terms.negotiation_id,
            party_a: terms.party_a,
            party_b: terms.party_b,
            rent_refund: existing.rent_refund,
            a_owes: terms.a_owes,
            b_owes: terms.b_owes,
            a_funded: existing.a_funded,
            b_funded: existing.b_funded,
            expiry_unix_ts: terms.expiry_unix_ts,
        };
        if existing.negotiation_id != desired.negotiation_id
            || existing.party_a != desired.party_a
            || existing.party_b != desired.party_b
            || existing.a_owes != desired.a_owes
            || existing.b_owes != desired.b_owes
            || existing.expiry_unix_ts != desired.expiry_unix_ts
        {
            return Err(EscrowError::TermsMismatch.into());
        }
        return Ok(());
    }

    // Create escrow PDA account.
    let rent = Rent::from_account_info(rent_sysvar)?;
    let lamports = rent.minimum_balance(EscrowState::LEN);
    let create_ix = system_instruction::create_account(
        payer.key,
        escrow.key,
        lamports,
        EscrowState::LEN as u64,
        program_id,
    );
    invoke_signed(
        &create_ix,
        &[payer.clone(), escrow.clone(), system_program.clone()],
        &[&[
            ESCROW_SEED_PREFIX,
            &terms.negotiation_id,
            terms.party_a.as_ref(),
            terms.party_b.as_ref(),
            &[bump],
        ]],
    )?;

    // Initialize state.
    let state = EscrowState {
        version: EscrowState::VERSION,
        bump,
        negotiation_id: terms.negotiation_id,
        party_a: terms.party_a,
        party_b: terms.party_b,
        rent_refund: *payer.key,
        a_owes: terms.a_owes,
        b_owes: terms.b_owes,
        a_funded: false,
        b_funded: false,
        expiry_unix_ts: terms.expiry_unix_ts,
    };
    state.serialize(&mut &mut escrow.data.borrow_mut()[..])?;

    // Create token vault ATAs if required.
    if terms.a_owes.kind == LegKind::SplToken {
        let mint = next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?;
        let vault = next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?;
        if mint.key != &terms.a_owes.mint {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_vault = spl_associated_token_account::get_associated_token_address(escrow.key, mint.key);
        if &expected_vault != vault.key {
            return Err(EscrowError::TermsMismatch.into());
        }
        create_ata_if_missing(payer, vault, escrow, mint, system_program, token_program, ata_program)?;
    }
    if terms.b_owes.kind == LegKind::SplToken {
        let mint = next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?;
        let vault = next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?;
        if mint.key != &terms.b_owes.mint {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_vault = spl_associated_token_account::get_associated_token_address(escrow.key, mint.key);
        if &expected_vault != vault.key {
            return Err(EscrowError::TermsMismatch.into());
        }
        create_ata_if_missing(payer, vault, escrow, mint, system_program, token_program, ata_program)?;
    }

    Ok(())
}

fn create_ata_if_missing<'a>(
    payer: &AccountInfo<'a>,
    ata: &AccountInfo<'a>,
    owner: &AccountInfo<'a>,
    mint: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    ata_program: &AccountInfo<'a>,
) -> ProgramResult {
    if !ata.data_is_empty() {
        return Ok(());
    }
    if ata_program.key != &spl_associated_token_account::id() {
        return Err(EscrowError::InvalidInstruction.into());
    }

    let ix = spl_associated_token_account::instruction::create_associated_token_account(
        payer.key,
        owner.key,
        mint.key,
        token_program.key,
    );

    invoke(
        &ix,
        &[
            payer.clone(),
            ata.clone(),
            owner.clone(),
            mint.clone(),
            system_program.clone(),
            token_program.clone(),
        ],
    )?;
    Ok(())
}

fn fund(program_id: &Pubkey, accounts: &[AccountInfo], party: EscrowParty) -> ProgramResult {
    let mut ai = accounts.iter();

    let funder = next_account_info(&mut ai)?;
    let escrow = next_account_info(&mut ai)?;
    let party_a = next_account_info(&mut ai)?;
    let party_b = next_account_info(&mut ai)?;
    let rent_refund = next_account_info(&mut ai)?;
    let system_program = next_account_info(&mut ai)?;
    let token_program = next_account_info(&mut ai)?;

    if !funder.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if token_program.key != &spl_token::id() {
        return Err(EscrowError::InvalidTokenProgram.into());
    }
    if escrow.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    let mut state =
        EscrowState::try_from_slice(&escrow.data.borrow()).map_err(|_| EscrowError::InvalidInstruction)?;

    // Verify expected accounts.
    let (expected_escrow, _bump) =
        derive_escrow_pda(program_id, &state.negotiation_id, &state.party_a, &state.party_b);
    if &expected_escrow != escrow.key {
        return Err(EscrowError::InvalidPda.into());
    }
    if party_a.key != &state.party_a || party_b.key != &state.party_b {
        return Err(EscrowError::InvalidParty.into());
    }
    if rent_refund.key != &state.rent_refund {
        return Err(EscrowError::TermsMismatch.into());
    }

    let now = Clock::get()?.unix_timestamp;
    if now > state.expiry_unix_ts {
        return Err(EscrowError::EscrowExpired.into());
    }

    let (owed_leg, funded_flag, expected_funder) = match party {
        EscrowParty::PartyA => (&state.a_owes, &mut state.a_funded, &state.party_a),
        EscrowParty::PartyB => (&state.b_owes, &mut state.b_funded, &state.party_b),
    };
    if funder.key != expected_funder {
        return Err(EscrowError::InvalidParty.into());
    }
    if *funded_flag {
        return Err(EscrowError::AlreadyFunded.into());
    }

    // Parse any token-leg accounts needed for settlement.
    let mut a_vault: Option<&AccountInfo> = None;
    let mut a_recipient: Option<&AccountInfo> = None;
    if state.a_owes.kind == LegKind::SplToken {
        a_vault = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        a_recipient = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        let expected_vault =
            spl_associated_token_account::get_associated_token_address(escrow.key, &state.a_owes.mint);
        if a_vault.unwrap().key != &expected_vault {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_recipient =
            spl_associated_token_account::get_associated_token_address(&state.party_b, &state.a_owes.mint);
        if a_recipient.unwrap().key != &expected_recipient {
            return Err(EscrowError::TermsMismatch.into());
        }
    }
    let mut b_vault: Option<&AccountInfo> = None;
    let mut b_recipient: Option<&AccountInfo> = None;
    if state.b_owes.kind == LegKind::SplToken {
        b_vault = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        b_recipient = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        let expected_vault =
            spl_associated_token_account::get_associated_token_address(escrow.key, &state.b_owes.mint);
        if b_vault.unwrap().key != &expected_vault {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_recipient =
            spl_associated_token_account::get_associated_token_address(&state.party_a, &state.b_owes.mint);
        if b_recipient.unwrap().key != &expected_recipient {
            return Err(EscrowError::TermsMismatch.into());
        }
    }

    // Perform funding transfer.
    match owed_leg.kind {
        LegKind::NativeSol => {
            let ix = system_instruction::transfer(funder.key, escrow.key, owed_leg.amount);
            invoke(&ix, &[funder.clone(), escrow.clone(), system_program.clone()])?;
        }
        LegKind::SplToken => {
            let source = next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?;
            verify_source_token_account(source, token_program, funder.key, &owed_leg.mint)?;

            let vault = match party {
                EscrowParty::PartyA => a_vault.ok_or(EscrowError::MissingAccounts)?,
                EscrowParty::PartyB => b_vault.ok_or(EscrowError::MissingAccounts)?,
            };

            let ix = spl_token::instruction::transfer(
                token_program.key,
                source.key,
                vault.key,
                funder.key,
                &[],
                owed_leg.amount,
            )?;
            invoke(&ix, &[source.clone(), vault.clone(), funder.clone(), token_program.clone()])?;
        }
    }

    *funded_flag = true;
    state.serialize(&mut &mut escrow.data.borrow_mut()[..])?;

    // Auto-settle on second fund.
    if state.a_funded && state.b_funded {
        msg!("Escrow fully funded; settling");
        settle(program_id, escrow, &state, party_a, party_b, rent_refund, token_program, a_vault, a_recipient, b_vault, b_recipient)?;
    }

    Ok(())
}

fn verify_source_token_account<'a>(
    source: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    expected_owner: &Pubkey,
    expected_mint: &Pubkey,
) -> Result<(), ProgramError> {
    if source.owner != token_program.key {
        return Err(EscrowError::InvalidTokenAccount.into());
    }
    let account = spl_token::state::Account::unpack(&source.data.borrow())
        .map_err(|_| EscrowError::InvalidTokenAccount)?;
    if &account.owner != expected_owner || &account.mint != expected_mint {
        return Err(EscrowError::InvalidTokenAccount.into());
    }
    Ok(())
}

fn settle<'a>(
    program_id: &Pubkey,
    escrow: &AccountInfo<'a>,
    state: &EscrowState,
    party_a: &AccountInfo<'a>,
    party_b: &AccountInfo<'a>,
    rent_refund: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    a_vault: Option<&AccountInfo<'a>>,
    a_recipient: Option<&AccountInfo<'a>>,
    b_vault: Option<&AccountInfo<'a>>,
    b_recipient: Option<&AccountInfo<'a>>,
) -> ProgramResult {
    // Pay A's leg to party B.
    match state.a_owes.kind {
        LegKind::NativeSol => {
            transfer_lamports(escrow, party_b, state.a_owes.amount)?;
        }
        LegKind::SplToken => {
            let vault = a_vault.ok_or(EscrowError::MissingAccounts)?;
            let recipient = a_recipient.ok_or(EscrowError::MissingAccounts)?;
            token_transfer_from_escrow(program_id, escrow, token_program, vault, recipient, state.a_owes.amount, state)?;
            token_close_from_escrow(program_id, escrow, token_program, vault, rent_refund, state)?;
        }
    }

    // Pay B's leg to party A.
    match state.b_owes.kind {
        LegKind::NativeSol => {
            transfer_lamports(escrow, party_a, state.b_owes.amount)?;
        }
        LegKind::SplToken => {
            let vault = b_vault.ok_or(EscrowError::MissingAccounts)?;
            let recipient = b_recipient.ok_or(EscrowError::MissingAccounts)?;
            token_transfer_from_escrow(program_id, escrow, token_program, vault, recipient, state.b_owes.amount, state)?;
            token_close_from_escrow(program_id, escrow, token_program, vault, rent_refund, state)?;
        }
    }

    // Close escrow state: refund remaining lamports (rent) to rent_refund.
    close_account(escrow, rent_refund)?;
    Ok(())
}

fn token_transfer_from_escrow<'a>(
    program_id: &Pubkey,
    escrow: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    source_vault: &AccountInfo<'a>,
    dest: &AccountInfo<'a>,
    amount: u64,
    state: &EscrowState,
) -> ProgramResult {
    let ix = spl_token::instruction::transfer(
        token_program.key,
        source_vault.key,
        dest.key,
        escrow.key,
        &[],
        amount,
    )?;

    let seeds: [&[u8]; 5] = [
        ESCROW_SEED_PREFIX,
        &state.negotiation_id,
        state.party_a.as_ref(),
        state.party_b.as_ref(),
        &[state.bump],
    ];
    invoke_signed(
        &ix,
        &[
            source_vault.clone(),
            dest.clone(),
            escrow.clone(),
            token_program.clone(),
        ],
        &[&seeds],
    )?;
    Ok(())
}

fn token_close_from_escrow<'a>(
    program_id: &Pubkey,
    escrow: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    vault: &AccountInfo<'a>,
    destination: &AccountInfo<'a>,
    state: &EscrowState,
) -> ProgramResult {
    let ix = spl_token::instruction::close_account(
        token_program.key,
        vault.key,
        destination.key,
        escrow.key,
        &[],
    )?;

    let seeds: [&[u8]; 5] = [
        ESCROW_SEED_PREFIX,
        &state.negotiation_id,
        state.party_a.as_ref(),
        state.party_b.as_ref(),
        &[state.bump],
    ];
    invoke_signed(
        &ix,
        &[vault.clone(), destination.clone(), escrow.clone(), token_program.clone()],
        &[&seeds],
    )?;
    Ok(())
}

fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> ProgramResult {
    if amount == 0 {
        return Ok(());
    }
    let from_lamports = **from.lamports.borrow();
    if from_lamports < amount {
        return Err(ProgramError::InsufficientFunds);
    }
    **from.lamports.borrow_mut() = from_lamports - amount;
    let to_lamports = **to.lamports.borrow();
    **to.lamports.borrow_mut() = to_lamports.saturating_add(amount);
    Ok(())
}

fn close_account(account: &AccountInfo, destination: &AccountInfo) -> ProgramResult {
    let lamports = **account.lamports.borrow();
    **account.lamports.borrow_mut() = 0;
    let dest_lamports = **destination.lamports.borrow();
    **destination.lamports.borrow_mut() = dest_lamports.saturating_add(lamports);
    Ok(())
}

fn cancel(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let mut ai = accounts.iter();
    let caller = next_account_info(&mut ai)?;
    let escrow = next_account_info(&mut ai)?;
    let party_a = next_account_info(&mut ai)?;
    let party_b = next_account_info(&mut ai)?;
    let rent_refund = next_account_info(&mut ai)?;
    let _system_program = next_account_info(&mut ai)?;
    let token_program = next_account_info(&mut ai)?;

    if !caller.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if token_program.key != &spl_token::id() {
        return Err(EscrowError::InvalidTokenProgram.into());
    }
    if escrow.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    let state =
        EscrowState::try_from_slice(&escrow.data.borrow()).map_err(|_| EscrowError::InvalidInstruction)?;
    if party_a.key != &state.party_a || party_b.key != &state.party_b {
        return Err(EscrowError::InvalidParty.into());
    }
    if rent_refund.key != &state.rent_refund {
        return Err(EscrowError::TermsMismatch.into());
    }
    if caller.key != &state.party_a && caller.key != &state.party_b {
        return Err(EscrowError::InvalidParty.into());
    }

    // Refund funded legs (only possible while not fully funded).
    if state.a_funded && state.b_funded {
        return Err(EscrowError::NotCancelable.into());
    }

    // Token vault accounts needed for refunds/closes (if token legs).
    let mut a_vault: Option<&AccountInfo> = None;
    let mut a_refund: Option<&AccountInfo> = None;
    if state.a_owes.kind == LegKind::SplToken {
        a_vault = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        a_refund = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        let expected_vault =
            spl_associated_token_account::get_associated_token_address(escrow.key, &state.a_owes.mint);
        if a_vault.unwrap().key != &expected_vault {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_refund =
            spl_associated_token_account::get_associated_token_address(&state.party_a, &state.a_owes.mint);
        if a_refund.unwrap().key != &expected_refund {
            return Err(EscrowError::TermsMismatch.into());
        }
    }
    let mut b_vault: Option<&AccountInfo> = None;
    let mut b_refund: Option<&AccountInfo> = None;
    if state.b_owes.kind == LegKind::SplToken {
        b_vault = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        b_refund = Some(next_account_info(&mut ai).map_err(|_| EscrowError::MissingAccounts)?);
        let expected_vault =
            spl_associated_token_account::get_associated_token_address(escrow.key, &state.b_owes.mint);
        if b_vault.unwrap().key != &expected_vault {
            return Err(EscrowError::TermsMismatch.into());
        }
        let expected_refund =
            spl_associated_token_account::get_associated_token_address(&state.party_b, &state.b_owes.mint);
        if b_refund.unwrap().key != &expected_refund {
            return Err(EscrowError::TermsMismatch.into());
        }
    }

    if state.a_funded {
        match state.a_owes.kind {
            LegKind::NativeSol => transfer_lamports(escrow, party_a, state.a_owes.amount)?,
            LegKind::SplToken => {
                let vault = a_vault.ok_or(EscrowError::MissingAccounts)?;
                let refund = a_refund.ok_or(EscrowError::MissingAccounts)?;
                token_transfer_from_escrow(program_id, escrow, token_program, vault, refund, state.a_owes.amount, &state)?;
            }
        }
    }
    if state.b_funded {
        match state.b_owes.kind {
            LegKind::NativeSol => transfer_lamports(escrow, party_b, state.b_owes.amount)?,
            LegKind::SplToken => {
                let vault = b_vault.ok_or(EscrowError::MissingAccounts)?;
                let refund = b_refund.ok_or(EscrowError::MissingAccounts)?;
                token_transfer_from_escrow(program_id, escrow, token_program, vault, refund, state.b_owes.amount, &state)?;
            }
        }
    }

    // Close token vaults (regardless of funded; should be empty after refund).
    if state.a_owes.kind == LegKind::SplToken {
        let vault = a_vault.ok_or(EscrowError::MissingAccounts)?;
        token_close_from_escrow(program_id, escrow, token_program, vault, rent_refund, &state)?;
    }
    if state.b_owes.kind == LegKind::SplToken {
        let vault = b_vault.ok_or(EscrowError::MissingAccounts)?;
        token_close_from_escrow(program_id, escrow, token_program, vault, rent_refund, &state)?;
    }

    close_account(escrow, rent_refund)?;
    Ok(())
}
