import React from "react";
import { Account, useFetchAccountInfo } from "providers/accounts";
import {
  TokenAccount,
  MintAccountInfo,
  TokenAccountInfo,
  MultisigAccountInfo,
} from "validators/accounts/token";
import { create } from "superstruct";
import { TableCardBody } from "components/common/TableCardBody";
import { Address } from "components/common/Address";
import { UnknownAccountCard } from "./UnknownAccountCard";
import { Cluster, useCluster } from "providers/cluster";
import { abbreviatedNumber, normalizeTokenAmount } from "utils";
import { addressLabel } from "utils/tx";
import { reportError } from "utils/sentry";
import { useTokenRegistry } from "providers/mints/token-registry";
import { BigNumber } from "bignumber.js";
import { Copyable } from "components/common/Copyable";
import { CoingeckoStatus, useCoinGecko } from "utils/coingecko";
import { displayTimestampWithoutDate } from "utils/date";
import { LoadingCard } from "components/common/LoadingCard";

const getEthAddress = (link?: string) => {
  let address = "";
  if (link) {
    const extractEth = link.match(/0x[a-fA-F0-9]{40,64}/);

    if (extractEth) {
      address = extractEth[0];
    }
  }

  return address;
};

export function TokenAccountSection({
  account,
  tokenAccount,
}: {
  account: Account;
  tokenAccount: TokenAccount;
}) {
  const { cluster } = useCluster();

  try {
    switch (tokenAccount.type) {
      case "mint": {
        const info = create(tokenAccount.info, MintAccountInfo);
        return <MintAccountCard account={account} info={info} />;
      }
      case "account": {
        const info = create(tokenAccount.info, TokenAccountInfo);
        return <TokenAccountCard account={account} info={info} />;
      }
      case "multisig": {
        const info = create(tokenAccount.info, MultisigAccountInfo);
        return <MultisigAccountCard account={account} info={info} />;
      }
    }
  } catch (err) {
    if (cluster !== Cluster.Custom) {
      reportError(err, {
        address: account.pubkey.toBase58(),
      });
    }
  }
  return <UnknownAccountCard account={account} />;
}

function MintAccountCard({
  account,
  info,
}: {
  account: Account;
  info: MintAccountInfo;
}) {
  const { tokenRegistry } = useTokenRegistry();
  const mintAddress = account.pubkey.toBase58();
  const fetchInfo = useFetchAccountInfo();
  const refresh = () => fetchInfo(account.pubkey);
  const tokenInfo = tokenRegistry.get(mintAddress);

  const bridgeContractAddress = getEthAddress(
    tokenInfo?.extensions?.bridgeContract
  );
  const assetContractAddress = getEthAddress(
    tokenInfo?.extensions?.assetContract
  );

  const coinInfo = useCoinGecko(tokenInfo?.extensions?.coingeckoId);

  let tokenPriceInfo;
  if (coinInfo?.status === CoingeckoStatus.Success) {
    tokenPriceInfo = coinInfo.coinInfo;
  }

  return (
    <>
      {tokenInfo?.extensions?.coingeckoId &&
        coinInfo?.status === CoingeckoStatus.Loading && (
          <LoadingCard message="Loading token price data" />
        )}
      {tokenPriceInfo && (
        <div className="row">
          <div className="col-12 col-lg-4 col-xl">
            <div className="card">
              <div className="card-body">
                <h4>
                  Price{" "}
                  <span className="ml-2 badge badge-primary rank">
                    Rank #{tokenPriceInfo.market_cap_rank}
                  </span>
                </h4>
                <h1 className="mb-0">
                  ${tokenPriceInfo.price.toFixed(2)}{" "}
                  {tokenPriceInfo.price_change_percentage_24h > 0 && (
                    <small className="change-positive">
                      &uarr;{" "}
                      {tokenPriceInfo.price_change_percentage_24h.toFixed(2)}%
                    </small>
                  )}
                  {tokenPriceInfo.price_change_percentage_24h < 0 && (
                    <small className="change-negative">
                      &darr;{" "}
                      {tokenPriceInfo.price_change_percentage_24h.toFixed(2)}%
                    </small>
                  )}
                  {tokenPriceInfo.price_change_percentage_24h === 0 && (
                    <small>0%</small>
                  )}
                </h1>
              </div>
            </div>
          </div>
          <div className="col-12 col-lg-4 col-xl">
            <div className="card">
              <div className="card-body">
                <h4>24 Hour Volume</h4>
                <h1 className="mb-0">
                  ${abbreviatedNumber(tokenPriceInfo.volume_24)}
                </h1>
              </div>
            </div>
          </div>
          <div className="col-12 col-lg-4 col-xl">
            <div className="card">
              <div className="card-body">
                <h4>Market Cap</h4>
                <h1 className="mb-0">
                  ${abbreviatedNumber(tokenPriceInfo.market_cap)}
                </h1>
                <p className="updated-time text-muted">
                  Updated at{" "}
                  {displayTimestampWithoutDate(
                    tokenPriceInfo.last_updated.getTime()
                  )}
                </p>
              </div>
            </div>
          </div>
        </div>
      )}
      <div className="card">
        <div className="card-header">
          <h3 className="card-header-title mb-0 d-flex align-items-center">
            {tokenInfo ? "Overview" : "Token Mint"}
          </h3>
          <button className="btn btn-white btn-sm" onClick={refresh}>
            <span className="fe fe-refresh-cw mr-2"></span>
            Refresh
          </button>
        </div>
        <TableCardBody>
          <tr>
            <td>Address</td>
            <td className="text-lg-right">
              <Address pubkey={account.pubkey} alignRight raw />
            </td>
          </tr>
          <tr>
            <td>
              {info.mintAuthority === null ? "Fixed Supply" : "Current Supply"}
            </td>
            <td className="text-lg-right">
              {normalizeTokenAmount(info.supply, info.decimals).toLocaleString(
                "en-US",
                {
                  minimumFractionDigits: info.decimals,
                }
              )}
            </td>
          </tr>
          {tokenInfo?.extensions?.website && (
            <tr>
              <td>Website</td>
              <td className="text-lg-right">
                <a
                  rel="noopener noreferrer"
                  target="_blank"
                  href={tokenInfo.extensions.website}
                >
                  {tokenInfo.extensions.website}
                  <span className="fe fe-external-link ml-2"></span>
                </a>
              </td>
            </tr>
          )}
          {info.mintAuthority && (
            <tr>
              <td>Mint Authority</td>
              <td className="text-lg-right">
                <Address pubkey={info.mintAuthority} alignRight link />
              </td>
            </tr>
          )}
          {info.freezeAuthority && (
            <tr>
              <td>Freeze Authority</td>
              <td className="text-lg-right">
                <Address pubkey={info.freezeAuthority} alignRight link />
              </td>
            </tr>
          )}
          <tr>
            <td>Decimals</td>
            <td className="text-lg-right">{info.decimals}</td>
          </tr>
          {!info.isInitialized && (
            <tr>
              <td>Status</td>
              <td className="text-lg-right">Uninitialized</td>
            </tr>
          )}
          {tokenInfo?.extensions?.bridgeContract && bridgeContractAddress && (
            <tr>
              <td>Bridge Contract</td>
              <td className="text-lg-right">
                <Copyable text={bridgeContractAddress}>
                  <a
                    href={tokenInfo.extensions.bridgeContract}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {bridgeContractAddress}
                  </a>
                </Copyable>
              </td>
            </tr>
          )}
          {tokenInfo?.extensions?.assetContract && assetContractAddress && (
            <tr>
              <td>Bridged Asset Contract</td>
              <td className="text-lg-right">
                <Copyable text={assetContractAddress}>
                  <a
                    href={tokenInfo.extensions.bridgeContract}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {assetContractAddress}
                  </a>
                </Copyable>
              </td>
            </tr>
          )}
        </TableCardBody>
      </div>
    </>
  );
}

function TokenAccountCard({
  account,
  info,
}: {
  account: Account;
  info: TokenAccountInfo;
}) {
  const refresh = useFetchAccountInfo();
  const { cluster } = useCluster();
  const { tokenRegistry } = useTokenRegistry();
  const label = addressLabel(account.pubkey.toBase58(), cluster, tokenRegistry);

  let unit, balance;
  if (info.isNative) {
    unit = "SOL";
    balance = (
      <>
        ◎
        <span className="text-monospace">
          {new BigNumber(info.tokenAmount.uiAmountString).toFormat(9)}
        </span>
      </>
    );
  } else {
    balance = <>{info.tokenAmount.uiAmountString}</>;
    unit = tokenRegistry.get(info.mint.toBase58())?.symbol || "tokens";
  }

  return (
    <div className="card">
      <div className="card-header">
        <h3 className="card-header-title mb-0 d-flex align-items-center">
          Token Account
        </h3>
        <button
          className="btn btn-white btn-sm"
          onClick={() => refresh(account.pubkey)}
        >
          <span className="fe fe-refresh-cw mr-2"></span>
          Refresh
        </button>
      </div>

      <TableCardBody>
        <tr>
          <td>Address</td>
          <td className="text-lg-right">
            <Address pubkey={account.pubkey} alignRight raw />
          </td>
        </tr>
        {label && (
          <tr>
            <td>Address Label</td>
            <td className="text-lg-right">{label}</td>
          </tr>
        )}
        <tr>
          <td>Mint</td>
          <td className="text-lg-right">
            <Address pubkey={info.mint} alignRight link />
          </td>
        </tr>
        <tr>
          <td>Owner</td>
          <td className="text-lg-right">
            <Address pubkey={info.owner} alignRight link />
          </td>
        </tr>
        <tr>
          <td>Token balance ({unit})</td>
          <td className="text-lg-right">{balance}</td>
        </tr>
        {info.state === "uninitialized" && (
          <tr>
            <td>Status</td>
            <td className="text-lg-right">Uninitialized</td>
          </tr>
        )}
        {info.rentExemptReserve && (
          <tr>
            <td>Rent-exempt reserve (SOL)</td>
            <td className="text-lg-right">
              <>
                ◎
                <span className="text-monospace">
                  {new BigNumber(
                    info.rentExemptReserve.uiAmountString
                  ).toFormat(9)}
                </span>
              </>
            </td>
          </tr>
        )}
      </TableCardBody>
    </div>
  );
}

function MultisigAccountCard({
  account,
  info,
}: {
  account: Account;
  info: MultisigAccountInfo;
}) {
  const refresh = useFetchAccountInfo();

  return (
    <div className="card">
      <div className="card-header">
        <h3 className="card-header-title mb-0 d-flex align-items-center">
          Multisig Account
        </h3>
        <button
          className="btn btn-white btn-sm"
          onClick={() => refresh(account.pubkey)}
        >
          <span className="fe fe-refresh-cw mr-2"></span>
          Refresh
        </button>
      </div>

      <TableCardBody>
        <tr>
          <td>Address</td>
          <td className="text-lg-right">
            <Address pubkey={account.pubkey} alignRight raw />
          </td>
        </tr>
        <tr>
          <td>Required Signers</td>
          <td className="text-lg-right">{info.numRequiredSigners}</td>
        </tr>
        <tr>
          <td>Valid Signers</td>
          <td className="text-lg-right">{info.numValidSigners}</td>
        </tr>
        {info.signers.map((signer) => (
          <tr key={signer.toString()}>
            <td>Signer</td>
            <td className="text-lg-right">
              <Address pubkey={signer} alignRight link />
            </td>
          </tr>
        ))}
        {!info.isInitialized && (
          <tr>
            <td>Status</td>
            <td className="text-lg-right">Uninitialized</td>
          </tr>
        )}
      </TableCardBody>
    </div>
  );
}
