// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

/// Platforms without a save path: the caller shows "download in the web console".
Future<bool> saveCsv(List<int> bytes, String filename) async => false;
