#!/usr/bin/env python3
"""Soju service bridge. Account library snapshots are accepted only on stdin."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import sys
from library_services import (ServiceError, search, steam_owned, gog_owned, gog_users,
                              save_account, account_path)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['search', 'sync', 'forget', 'gog-users'])
    parser.add_argument('value', nargs='?', default='')
    parser.add_argument('--country', default='US')
    parser.add_argument('--base', default=os.environ.get('SOJU_BASE', str(Path.home()/'.battlenet-macos')))
    args = parser.parse_args()
    try:
        if args.action == 'search':
            result = search(args.value, args.country)
        elif args.action == 'gog-users':
            result = {'users': gog_users(args.base)}
        elif args.action == 'forget':
            account_path(args.base, args.value).unlink(missing_ok=True)
            result = {'message': 'Account import removed. Installed games are kept.'}
        else:
            payload = sys.stdin.read(8 * 1024 * 1024 + 1)
            if len(payload) > 8 * 1024 * 1024:
                raise ServiceError('Account settings are too large.')
            settings = json.loads(payload or '{}')
            if not isinstance(settings, dict):
                raise ServiceError('Invalid account settings.')
            if args.value == 'steam':
                games, label = steam_owned(settings)
            elif args.value == 'gog':
                games, label = gog_owned(args.base, settings)
            else:
                raise ServiceError('Unsupported account provider.')
            result = save_account(args.base, args.value, games, label)
        print(json.dumps(result, ensure_ascii=False))
    except ServiceError as error:
        print(json.dumps({'error': str(error)}))
        return 1
    except sqlite3.Error:
        print(json.dumps({'error': 'Galaxy library is busy or its format changed. Let Galaxy finish loading, then retry.'}))
        return 1
    except Exception:
        # Do not include raw exceptions, response bodies or credentials in logs.
        print(json.dumps({'error': 'Could not complete this request. Your previous account import is unchanged.'}))
        return 1
    return 0

if __name__ == '__main__':
    sys.exit(main())
