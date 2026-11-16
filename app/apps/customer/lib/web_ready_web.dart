// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:web/web.dart' as web;

/// Web: tells the page shell (web/index.html) that the first data is on screen.
void markAppReady() => web.window.dispatchEvent(web.Event('dahae-ready'));
