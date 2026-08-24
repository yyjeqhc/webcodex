using System;
using System.Globalization;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Host;
using System.Management.Automation.Runspaces;
using System.Text;

namespace WebCodexPersistentShell
{
    public sealed class UserHost : PSHost
    {
        private readonly PSHost inner;
        private readonly Guid instanceId = Guid.NewGuid();

        public UserHost(PSHost inner)
        {
            if (inner == null)
            {
                throw new ArgumentNullException("inner");
            }
            this.inner = inner;
        }

        public override Guid InstanceId
        {
            get { return instanceId; }
        }

        public override string Name
        {
            get { return "WebCodexPersistentShellUserHost"; }
        }

        public override Version Version
        {
            get { return inner.Version; }
        }

        public override PSHostUserInterface UI
        {
            get { return inner.UI; }
        }

        public override CultureInfo CurrentCulture
        {
            get { return inner.CurrentCulture; }
        }

        public override CultureInfo CurrentUICulture
        {
            get { return inner.CurrentUICulture; }
        }

        public override void SetShouldExit(int requestedExitCode)
        {
            // `exit N` is shell termination, not command completion. Terminate
            // the owned PowerShell process immediately so controller code can
            // never publish a completion frame for an exited user shell.
            Environment.Exit(requestedExitCode);
        }

        public override void EnterNestedPrompt()
        {
            inner.EnterNestedPrompt();
        }

        public override void ExitNestedPrompt()
        {
            inner.ExitNestedPrompt();
        }

        public override void NotifyBeginApplication()
        {
            inner.NotifyBeginApplication();
        }

        public override void NotifyEndApplication()
        {
            inner.NotifyEndApplication();
        }
    }

    public static class Controller
    {
        private const string StdoutMagic = "WCPSO1";
        private const string StderrMagic = "WCPSE1";
        private const string ControlMagic = "WCPS1";
        private const string StatusTag = "WebCodexPersistentShellCommandStatus";
        private const string StatusPostamble = "\nMicrosoft.PowerShell.Utility\\Write-Information -Tags 'WebCodexPersistentShellCommandStatus' -MessageData ([pscustomobject]@{ Ok = $?; Native = $LASTEXITCODE }) -InformationAction SilentlyContinue";
        private static readonly UTF8Encoding Utf8NoBom = new UTF8Encoding(false);
        private static readonly Encoding Ascii = Encoding.ASCII;

        public static void Run(PSHost outerHost)
        {
            Stream stdin = Console.OpenStandardInput();
            Stream stdout = Console.OpenStandardOutput();
            Stream stderr = Console.OpenStandardError();
            StreamReader reader = new StreamReader(stdin, Utf8NoBom, false, 4096, true);
            UserHost userHost = new UserHost(outerHost);
            Runspace userRunspace = RunspaceFactory.CreateRunspace(userHost);
            userRunspace.Open();

            try
            {
                string line;
                while ((line = reader.ReadLine()) != null)
                {
                    string[] parts = line.Split(new char[] { '\t' });
                    if (parts.Length != 3 || !ValidToken(parts[0]))
                    {
                        Environment.Exit(125);
                        return;
                    }

                    // Framing authority is deliberately confined to these stack locals.
                    // Neither the token nor the publication target is stored in a
                    // PowerShell variable, Runspace SessionState, host field, or static field.
                    string token = parts[0];
                    string source;
                    string controlPath;
                    try
                    {
                        source = Utf8NoBom.GetString(Convert.FromBase64String(parts[1]));
                        controlPath = Utf8NoBom.GetString(Convert.FromBase64String(parts[2]));
                    }
                    catch (Exception)
                    {
                        Environment.Exit(125);
                        return;
                    }
                    if (String.IsNullOrEmpty(controlPath))
                    {
                        Environment.Exit(125);
                        return;
                    }

                    int status = InvokeUserCommand(userRunspace, source, stderr);

                    // These writes are reachable only after InvokeUserCommand returned.
                    // User PowerShell can write marker-like bytes, but it cannot learn
                    // the active token/control target through shell-language state.
                    WriteSync(stdout, StdoutMagic, token);
                    WriteSync(stderr, StderrMagic, token);

                    string cwd;
                    try
                    {
                        cwd = userRunspace.SessionStateProxy.Path.CurrentLocation.ProviderPath;
                    }
                    catch (Exception)
                    {
                        Environment.Exit(125);
                        return;
                    }
                    if (String.IsNullOrEmpty(cwd))
                    {
                        Environment.Exit(125);
                        return;
                    }

                    string controlText = ControlMagic + '\0' + token + '\0' + status.ToString(CultureInfo.InvariantCulture) + '\0' + cwd + '\0';
                    string temporaryControlPath = controlPath + ".tmp";
                    try
                    {
                        File.WriteAllBytes(temporaryControlPath, Utf8NoBom.GetBytes(controlText));
                        File.Move(temporaryControlPath, controlPath);
                    }
                    catch (Exception)
                    {
                        try { File.Delete(temporaryControlPath); } catch (Exception) { }
                        Environment.Exit(125);
                        return;
                    }
                }
            }
            finally
            {
                try { userRunspace.Close(); } catch (Exception) { }
                userRunspace.Dispose();
                reader.Dispose();
            }
        }

        private static int InvokeUserCommand(Runspace userRunspace, string source, Stream stderr)
        {
            int status = 0;
            using (PowerShell shell = PowerShell.Create())
            {
                shell.Runspace = userRunspace;
                try
                {
                    // Match the prior Windows transport's per-command native-status reset.
                    userRunspace.SessionStateProxy.SetVariable("LASTEXITCODE", 0);
                    // Capture the same final `$?` / `$LASTEXITCODE` pair used by
                    // the original Windows wrapper. Information is a separate
                    // PowerShell stream and is silenced from user output; it is
                    // command-status metadata only and never participates in
                    // completion authentication.
                    shell.AddScript(source + StatusPostamble, false);
                    shell.AddCommand("Out-Default");
                    shell.Invoke();
                }
                catch (Exception error)
                {
                    status = 1;
                    if (shell.Streams.Error.Count == 0)
                    {
                        WriteErrorLine(stderr, error.ToString());
                    }
                }

                foreach (ErrorRecord errorRecord in shell.Streams.Error)
                {
                    WriteErrorLine(stderr, errorRecord.ToString());
                }

                if (status == 0)
                {
                    int capturedStatus;
                    if (TryReadCommandStatus(shell, out capturedStatus))
                    {
                        status = capturedStatus;
                    }
                    else if (shell.HadErrors)
                    {
                        // Parse/terminating failures can skip the trusted
                        // postamble. Preserve the previous fail-closed status.
                        status = 1;
                    }
                }
            }
            return status;
        }

        private static bool TryReadCommandStatus(PowerShell shell, out int status)
        {
            for (int index = shell.Streams.Information.Count - 1; index >= 0; index--)
            {
                InformationRecord record = shell.Streams.Information[index];
                bool tagged = false;
                foreach (string tag in record.Tags)
                {
                    if (String.Equals(tag, StatusTag, StringComparison.Ordinal))
                    {
                        tagged = true;
                        break;
                    }
                }
                if (!tagged)
                {
                    continue;
                }

                try
                {
                    PSObject data = PSObject.AsPSObject(record.MessageData);
                    PSPropertyInfo okProperty = data.Properties["Ok"];
                    PSPropertyInfo nativeProperty = data.Properties["Native"];
                    if (okProperty == null || nativeProperty == null)
                    {
                        continue;
                    }
                    bool ok = LanguagePrimitives.ConvertTo<bool>(okProperty.Value);
                    int nativeExitCode = nativeProperty.Value == null
                        ? 0
                        : LanguagePrimitives.ConvertTo<int>(nativeProperty.Value);
                    status = ok ? 0 : (nativeExitCode != 0 ? nativeExitCode : 1);
                    return true;
                }
                catch (Exception)
                {
                    continue;
                }
            }

            status = 0;
            return false;
        }

        private static void WriteSync(Stream stream, string magic, string token)
        {
            byte[] bytes = Ascii.GetBytes(magic + '\0' + token + '\0');
            stream.Write(bytes, 0, bytes.Length);
            stream.Flush();
        }

        private static void WriteErrorLine(Stream stderr, string text)
        {
            byte[] bytes = Utf8NoBom.GetBytes((text ?? String.Empty) + Environment.NewLine);
            stderr.Write(bytes, 0, bytes.Length);
            stderr.Flush();
        }

        private static bool ValidToken(string token)
        {
            if (token == null || token.Length != 32)
            {
                return false;
            }
            for (int index = 0; index < token.Length; index++)
            {
                char value = token[index];
                if (!((value >= '0' && value <= '9') || (value >= 'a' && value <= 'f')))
                {
                    return false;
                }
            }
            return true;
        }
    }
}
