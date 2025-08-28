using System;
using System.Security.Cryptography;

namespace Cliptoo.Core.Services
{
    public static class HashingUtils
    {
        public static ulong ComputeHash(byte[] data)
        {
            var hashBytes = SHA256.HashData(data);
            return BitConverter.ToUInt64(hashBytes, 0);
        }
    }
}