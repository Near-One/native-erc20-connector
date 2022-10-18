pub async fn ft_balance_of(
    viewer: &workspaces::Account,
    token: &workspaces::AccountId,
    user: &workspaces::AccountId,
) -> anyhow::Result<u128> {
    let outcome = viewer
        .view(
            token,
            "ft_balance_of",
            serde_json::to_vec(&AccountIdArgs { account_id: user }).unwrap(),
        )
        .await?;
    let result: String = outcome.json()?;
    Ok(result.parse()?)
}

#[derive(serde::Serialize)]
pub struct AccountIdArgs<'a> {
    pub account_id: &'a workspaces::AccountId,
}
