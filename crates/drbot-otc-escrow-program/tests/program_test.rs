use drbot_otc_escrow_program::{
    derive_escrow_pda, EscrowInstruction, EscrowParty, EscrowTerms, Leg, LegKind,
};
use solana_program_test::ProgramTest;
use solana_sdk::account::ReadableAccount;
use solana_sdk::program_pack::Pack;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::system_instruction as sys_ix;
use solana_sdk::transaction::Transaction;
use solana_sdk::{pubkey::Pubkey, system_program};

fn usdc_decimals() -> u8 {
    6
}

#[tokio::test]
async fn escrow_sol_for_spl_token_autosettles_on_second_fund() {
    let program_id = Pubkey::new_unique();

    let mut pt = ProgramTest::new(
        "drbot_otc_escrow_program",
        program_id,
        solana_program_test::processor!(drbot_otc_escrow_program::process_instruction),
    );
    pt.add_program(
        "spl_token",
        spl_token::id(),
        solana_program_test::processor!(spl_token::processor::Processor::process),
    );
    pt.add_program(
        "spl_associated_token_account",
        spl_associated_token_account::id(),
        solana_program_test::processor!(spl_associated_token_account::processor::process_instruction),
    );

    let mut ctx = pt.start_with_context().await;
    let payer = Keypair::from_bytes(&ctx.payer.to_bytes()).unwrap();

    // Parties.
    let party_a = Keypair::new();
    let party_b = Keypair::new();

    // Fund parties with SOL for fees and the SOL leg.
    let fund_tx = Transaction::new_signed_with_payer(
        &[
            sys_ix::transfer(&payer.pubkey(), &party_a.pubkey(), 2_000_000_000),
            sys_ix::transfer(&payer.pubkey(), &party_b.pubkey(), 3_000_000_000),
        ],
        Some(&payer.pubkey()),
        &[&payer],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(fund_tx).await.unwrap();

    // Create a fake USDC mint and ATAs.
    let usdc_mint = Keypair::new();
    create_mint(
        &mut ctx,
        &usdc_mint,
        &payer,
        usdc_decimals(),
        &payer.pubkey(),
    )
    .await;

    let a_usdc_ata =
        spl_associated_token_account::get_associated_token_address(&party_a.pubkey(), &usdc_mint.pubkey());
    let b_usdc_ata =
        spl_associated_token_account::get_associated_token_address(&party_b.pubkey(), &usdc_mint.pubkey());

    create_ata(&mut ctx, &payer, &party_a.pubkey(), &usdc_mint.pubkey()).await;
    create_ata(&mut ctx, &payer, &party_b.pubkey(), &usdc_mint.pubkey()).await;

    // Mint USDC to party A (party A will pay token leg).
    mint_to(
        &mut ctx,
        &usdc_mint.pubkey(),
        &payer,
        &a_usdc_ata,
        250_000_000, // 250 USDC in micros
    )
    .await;

    // Terms: A pays 150 USDC, B pays 1 SOL.
    let negotiation_id = [7u8; 16];
    let terms = EscrowTerms {
        negotiation_id,
        party_a: party_a.pubkey(),
        party_b: party_b.pubkey(),
        a_owes: Leg::spl_token(usdc_mint.pubkey(), 150_000_000),
        b_owes: Leg::native_sol(1_000_000_000),
        expiry_unix_ts: i64::MAX,
    };

    let (escrow_pda, _bump) = derive_escrow_pda(&program_id, &terms.negotiation_id, &terms.party_a, &terms.party_b);
    let vault_ata =
        spl_associated_token_account::get_associated_token_address(&escrow_pda, &usdc_mint.pubkey());

    // Create escrow (payer can be either party; use party A).
    let create_ix = solana_sdk::instruction::Instruction {
        program_id,
        accounts: vec![
            solana_sdk::instruction::AccountMeta::new(party_a.pubkey(), true),
            solana_sdk::instruction::AccountMeta::new(escrow_pda, false),
            solana_sdk::instruction::AccountMeta::new_readonly(party_a.pubkey(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(party_b.pubkey(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(system_program::id(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(spl_token::id(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(solana_sdk::sysvar::rent::id(), false),
            solana_sdk::instruction::AccountMeta::new_readonly(usdc_mint.pubkey(), false),
            solana_sdk::instruction::AccountMeta::new(vault_ata, false),
        ],
        data: borsh::to_vec(&EscrowInstruction::CreateEscrow { terms }).unwrap(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&party_a.pubkey()),
        &[&party_a],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();

    // Fund party B first (SOL leg).
    let fund_b_ix = fund_ix(
        program_id,
        escrow_pda,
        &party_b.pubkey(),
        &party_a.pubkey(),
        &party_b.pubkey(),
        &party_a.pubkey(), // rent refund = party A
        &terms,
        Some((&vault_ata, &b_usdc_ata)),
        None,
        EscrowParty::PartyB,
    );
    let tx = Transaction::new_signed_with_payer(
        &[fund_b_ix],
        Some(&party_b.pubkey()),
        &[&party_b],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();

    let a_lamports_before = ctx
        .banks_client
        .get_account(party_a.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports();

    // Fund party A second (token leg) => should auto-settle.
    let fund_a_ix = fund_ix(
        program_id,
        escrow_pda,
        &party_a.pubkey(),
        &party_a.pubkey(),
        &party_b.pubkey(),
        &party_a.pubkey(),
        &terms,
        Some((&vault_ata, &b_usdc_ata)),
        Some(&a_usdc_ata),
        EscrowParty::PartyA,
    );
    let tx = Transaction::new_signed_with_payer(
        &[fund_a_ix],
        Some(&party_a.pubkey()),
        &[&party_a],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();

    // Escrow PDA should be closed.
    let escrow_acct = ctx.banks_client.get_account(escrow_pda).await.unwrap();
    assert!(escrow_acct.is_none());

    // Token vault should be closed.
    let vault_acct = ctx.banks_client.get_account(vault_ata).await.unwrap();
    assert!(vault_acct.is_none());

    // Party A received ~1 SOL (minus fees). Party A also paid rent but got it refunded on close.
    let a_lamports_after = ctx
        .banks_client
        .get_account(party_a.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports();
    assert!(a_lamports_after >= a_lamports_before + 999_000_000);

    // Party B received 150 USDC.
    let b_usdc = token_balance(&mut ctx, &b_usdc_ata).await;
    assert_eq!(b_usdc, 150_000_000);

    // Party A paid 150 USDC.
    let a_usdc = token_balance(&mut ctx, &a_usdc_ata).await;
    assert_eq!(a_usdc, 100_000_000);
}

fn fund_ix(
    program_id: Pubkey,
    escrow: Pubkey,
    funder: &Pubkey,
    party_a: &Pubkey,
    party_b: &Pubkey,
    rent_refund: &Pubkey,
    terms: &EscrowTerms,
    a_token_accounts: Option<(&Pubkey, &Pubkey)>,
    token_source: Option<&Pubkey>,
    party: EscrowParty,
) -> solana_sdk::instruction::Instruction {
    let mut accounts = vec![
        solana_sdk::instruction::AccountMeta::new(*funder, true),
        solana_sdk::instruction::AccountMeta::new(escrow, false),
        solana_sdk::instruction::AccountMeta::new(*party_a, false),
        solana_sdk::instruction::AccountMeta::new(*party_b, false),
        solana_sdk::instruction::AccountMeta::new(*rent_refund, false),
        solana_sdk::instruction::AccountMeta::new_readonly(system_program::id(), false),
        solana_sdk::instruction::AccountMeta::new_readonly(spl_token::id(), false),
    ];

    // Provide vault + recipient for any token legs.
    if terms.a_owes.kind == LegKind::SplToken {
        let (vault, recipient) = a_token_accounts.expect("a token accounts");
        accounts.push(solana_sdk::instruction::AccountMeta::new(*vault, false));
        accounts.push(solana_sdk::instruction::AccountMeta::new(*recipient, false));
    }

    // No B token leg in this test.

    // Provide source token account if funding token leg.
    if let Some(source) = token_source {
        accounts.push(solana_sdk::instruction::AccountMeta::new(*source, false));
    }

    solana_sdk::instruction::Instruction {
        program_id,
        accounts,
        data: borsh::to_vec(&EscrowInstruction::Fund { party }).unwrap(),
    }
}

async fn create_mint(
    ctx: &mut solana_program_test::ProgramTestContext,
    mint: &Keypair,
    payer: &Keypair,
    decimals: u8,
    mint_authority: &Pubkey,
) {
    let rent = ctx.banks_client.get_rent().await.unwrap();
    let mint_space = spl_token::state::Mint::LEN;
    let mint_lamports = rent.minimum_balance(mint_space);

    let create = sys_ix::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        mint_lamports,
        mint_space as u64,
        &spl_token::id(),
    );
    let init = spl_token::instruction::initialize_mint(
        &spl_token::id(),
        &mint.pubkey(),
        mint_authority,
        None,
        decimals,
    )
    .unwrap();

    let tx = Transaction::new_signed_with_payer(
        &[create, init],
        Some(&payer.pubkey()),
        &[payer, mint],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();
}

async fn create_ata(
    ctx: &mut solana_program_test::ProgramTestContext,
    payer: &Keypair,
    owner: &Pubkey,
    mint: &Pubkey,
) {
    let ix = spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &spl_token::id(),
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();
}

async fn mint_to(
    ctx: &mut solana_program_test::ProgramTestContext,
    mint: &Pubkey,
    mint_authority: &Keypair,
    dest: &Pubkey,
    amount: u64,
) {
    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        dest,
        &mint_authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&mint_authority.pubkey()),
        &[mint_authority],
        ctx.last_blockhash,
    );
    ctx.banks_client.process_transaction(tx).await.unwrap();
}

async fn token_balance(
    ctx: &mut solana_program_test::ProgramTestContext,
    account: &Pubkey,
) -> u64 {
    let acct = ctx.banks_client.get_account(*account).await.unwrap().unwrap();
    let token = spl_token::state::Account::unpack(acct.data()).unwrap();
    token.amount
}
