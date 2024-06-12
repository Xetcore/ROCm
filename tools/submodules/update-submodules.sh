#!/usr/bin/bash

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color


printHelp() {
	# Print the help message
	printf -- \
"
  Usage: update-submodules.sh [-t <tag>] [-p <path>]
  Description: Update submodules to the ROCm release tag
  Options:
	-h, --help: Print the help message
	-t, --tag: The ROCm release tag to update the submodules to
	-p, --path: The path to the submodules directory

  Example:
	./update-submodules.sh -t rocm-6.1.0 -p ../../libs"
}

updateSubmodules() {
	# Update the submodules in the given directory to the desired tag
	# $1: The directory to update the submodules in

	local directory=$1
	pushd "$directory" >> /dev/null || exit

	for d in */
	do
		pushd "$d" >> /dev/null || exit

		if [[ -e .git ]]; then
			echo -e "${GREEN}${d} is a git folder${NC}"

			if ! git fetch --all; then
				echo -e "${RED}Failed to fetch for ${d}${NC}"

				# Save the current directory to an array to output the list of
				#  failed fetches at the end.
				gitFetchErrors+=("$d")
			fi

			# Checkout the desired tag
			if ! git checkout tags/"${RocmRelease}"; then
				echo -e "${RED}Failed to checkout tag ${RocmRelease} for ${d}${NC}"

				# Save the current directory to an array to output the list of
				#  failed checkouts at the end.
				gitCheckoutErrors+=("$d")
			fi
		else
			echo -e "${RED}${d} is NOT a git folder${NC}"

			# Save the current directory to an array to output the list of
			#  non-git folders at the end.
			nonGitFolders+=("$d")
		fi
		echo -e "${NC}"
		popd >> /dev/null || exit
	done
	popd >> /dev/null || exit
}

#######
# MAIN PROGRAM
#######

# Default values
RocmRelease=rocm-6.1.0
PathToSubmodules=../../libs

# Parse command line parameters.
while [ $# -gt 0 ]; do
	case $1 in
		-h | --help )
			printHelp
			exit 0
			;;
		-t | --tag )
			shift
			RocmRelease=$1
			shift
			;;
		-p | --path )
			shift
			PathToSubmodules=$1
			shift
			;;
		* )
			shift
			;;
	esac
done

echo "********************************"
echo -e "${BLUE}Path to Submodules: ${PathToSubmodules}${NC}"
pushd "$PathToSubmodules" >> /dev/null || exit

echo -e "${BLUE}Syncing to Tag: ${RocmRelease}.${NC}"
echo "********************************"
echo

# Update submodules in the current directory
updateSubmodules "."

# Update the `openmp-extras` modules
updateSubmodules "openmp-extras"

popd >> /dev/null || exit

# Output summary of errors

echo           "********************************"
echo -e "${BLUE}***       Error Report       ***${NC}"
echo           "********************************"
echo

if [ ${#nonGitFolders[@]} -gt 0 ]; then
	echo -e "${RED}The following folders are not git folders:${NC}"

	for d in "${nonGitFolders[@]}"
	do
		echo "${d}"
	done
	echo
fi

if [ ${#gitFetchErrors[@]} -gt 0 ]; then
	echo -e "${RED}The following folders failed to fetch:${NC}"

	for d in "${gitFetchErrors[@]}"
	do
		echo "${d}"
	done
	echo
fi

if [ ${#gitCheckoutErrors[@]} -gt 0 ]; then
	echo -e "${RED}The following folders failed to checkout tag ${RocmRelease}:${NC}"

	for d in "${gitCheckoutErrors[@]}"
	do
		echo "${d}"
	done
	echo
fi
