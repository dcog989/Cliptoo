using System;
using System.Security.Cryptography;

namespace Cliptoo.Core.Services
{
    public static class HashingUtils
    {
        public static ulong ComputeHash(byte[] data)
        {
            using (var sha256 = SHA256.Create())
            {
                var hashBytes = sha256.ComputeHash(data);
                return BitConverter.ToUInt64(hashBytes, 0);
            }
        }
    }
}