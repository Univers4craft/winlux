# #2611 : put_UserAgent passe par user_agent_header avant WebKit.
# Le comportement est gardé par le test nommé.
ok((SCRIPTS / "test-webview-agent.py").is_file(),
   "test de l'agent utilisateur accepté par WebKit (#2611)")
