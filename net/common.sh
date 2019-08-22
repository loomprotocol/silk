# |source| this file
#
# Common utilities shared by other scripts in this directory
#
# The following directive disable complaints about unused variables in this
# file:
# shellcheck disable=2034
#

netDir=$(
  cd "$(dirname "${BASH_SOURCE[0]}")" || exit
  echo "$PWD"
)
netConfigDir="$netDir"/config
netLogDir="$netDir"/log
mkdir -p "$netConfigDir" "$netLogDir"

# shellcheck source=scripts/configure-metrics.sh
source "$(dirname "${BASH_SOURCE[0]}")"/../scripts/configure-metrics.sh

# shellcheck source=net/paths.sh
source "$(dirname "${BASH_SOURCE[0]}")"/paths.sh

configFile="$netConfigDir/config"
geoipConfigFile="$netConfigDir/geoip.yml"

entrypointIp=
publicNetwork=
netBasename=
sshPrivateKey=
letsEncryptDomainName=
externalNodeSshKey=
sshOptions=()
fullnodeIpList=()
fullnodeIpListPrivate=()
fullnodeIpListZone=()
clientIpList=()
clientIpListPrivate=()
clientIpListZone=()
blockstreamerIpList=()
blockstreamerIpListPrivate=()
blockstreamerIpListZone=()
replicatorIpList=()
replicatorIpListPrivate=()
replicatorIpListZone=()

buildSshOptions() {
  sshOptions=(
    -o "ConnectTimeout=20"
    -o "BatchMode=yes"
    -o "StrictHostKeyChecking=no"
    -o "UserKnownHostsFile=/dev/null"
    -o "User=solana"
    -o "IdentityFile=$sshPrivateKey"
    -o "LogLevel=ERROR"
  )

  [[ -z $externalNodeSshKey ]] || sshOptions+=(-o "IdentityFile=$externalNodeSshKey")
}

loadConfigFile() {
  [[ -r $configFile ]] || usage "Config file unreadable: $configFile"

  # shellcheck source=/dev/null
  source "$configFile"
  [[ -n "$publicNetwork" ]] || usage "Config file invalid, publicNetwork unspecified: $configFile"
  [[ -n "$netBasename" ]] || usage "Config file invalid, netBasename unspecified: $configFile"
  [[ -n $sshPrivateKey ]] || usage "Config file invalid, sshPrivateKey unspecified: $configFile"
  [[ ${#fullnodeIpList[@]} -gt 0 ]] || usage "Config file invalid, fullnodeIpList unspecified: $configFile"
  [[ ${#fullnodeIpListPrivate[@]} -gt 0 ]] || usage "Config file invalid, fullnodeIpListPrivate unspecified: $configFile"
  [[ ${#fullnodeIpList[@]} -eq ${#fullnodeIpListPrivate[@]} ]] || usage "Config file invalid, fullnodeIpList/fullnodeIpListPrivate length mismatch: $configFile"

  if $publicNetwork; then
    entrypointIp=${fullnodeIpList[0]}
  else
    entrypointIp=${fullnodeIpListPrivate[0]}
  fi

  buildSshOptions
  configureMetrics
}

# https://gist.github.com/cdown/1163649
urlencode() {
  declare s="$1"
  declare l=$((${#s} - 1))
  for i in $(seq 0 $l); do
    declare c="${s:$i:1}"
    case $c in
      [a-zA-Z0-9.~_-])
        echo -n "$c"
        ;;
      *)
        printf '%%%02X' "'$c"
        ;;
    esac
  done
}

