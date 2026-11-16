// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'social_auth.dart';

/// No native social SDK on this platform (web): the provider stays unconfigured and is hidden.
SocialSignIn? nativeSocialSignIn(String provider) => null;
