

import { useMemo, useState } from 'react';
import { useRouter } from 'expo-router';
import { View, Text, TextInput, Pressable, StyleSheet } from 'react-native';
import * as SecureStore from 'expo-secure-store';

type LoginResponse = {
  access_token: string;
  token_type: string;
}

type ErrorResponse = {
  error?: string;
}




export default function LoginPage() {

  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false);
  const apiBaseUrl = useMemo(
    () => process.env.EXPO_PUBLIC_API_URL ?? "http://192.168.1.74:3001",
    [],
  );
  const router = useRouter();
  const handleSubmit = async () => {
    setIsSubmitting(true);
    setErrorMessage(null);

    try {
      const response = await fetch(`${apiBaseUrl}/auth/login`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          username,
          password,
        }),
      });

      if (!response.ok) {
        const errorPayload = (await response.json().catch(() => null)) as
          | ErrorResponse
          | null;
        setErrorMessage(errorPayload?.error ?? "Login failed.");
        return;
      }

      const payload = (await response.json()) as LoginResponse;
      await SecureStore.setItemAsync("numble_token", payload.access_token);
      router.replace("/");
    } catch {
      setErrorMessage("Unable to reach backend. Check if it is running.");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <View style={styles.screen}>
      <View style={styles.card}>
        <Text style={styles.title}>Login</Text>
        <Text style={styles.subtitle}>Sign in to play Numble.</Text>
        <View style={styles.form}>
          <TextInput
            placeholder="Email"
            onChangeText={(text : string) => setUsername(text)}
            keyboardType="email-address"
            autoCapitalize="none"
            style={styles.input}
            placeholderTextColor="#94a3b8"
          />
          <TextInput
            placeholder="Password"
            onChangeText={(text : string) => setPassword(text)}
            secureTextEntry
            style={styles.input}
            placeholderTextColor="#94a3b8"
          />
        </View>
        {errorMessage ? <Text style={styles.error}>{errorMessage}</Text> : null}
        <Pressable
          style={[styles.button, isSubmitting && styles.buttonDisabled]}
          onPress={handleSubmit}
          disabled={isSubmitting}
        >
          <Text style={styles.buttonText}>{isSubmitting ? 'Logging in...' : 'Login'}</Text>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 24,
    paddingVertical: 40,
    backgroundColor: '#0f172a',
  },
  card: {
    width: '100%',
    maxWidth: 420,
    borderWidth: 1,
    borderColor: 'rgba(255,255,255,0.2)',
    borderRadius: 18,
    padding: 20,
    backgroundColor: 'rgba(2,6,23,0.85)',
  },
  title: {
    fontSize: 28,
    fontWeight: '700',
    color: '#ffffff',
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 14,
    color: '#cbd5e1',
    marginBottom: 16,
  },
  form: {
    gap: 12,
  },
  input: {
    width: '100%',
    borderRadius: 10,
    borderWidth: 1,
    borderColor: 'rgba(148,163,184,0.6)',
    backgroundColor: 'rgba(15,23,42,0.8)',
    color: '#ffffff',
    paddingHorizontal: 12,
    paddingVertical: 10,
  },
  error: {
    color: '#fca5a5',
    marginTop: 12,
    marginBottom: 8,
  },
  button: {
    marginTop: 8,
    borderRadius: 10,
    paddingVertical: 12,
    alignItems: 'center',
    backgroundColor: '#2563eb',
  },
  buttonDisabled: {
    opacity: 0.6,
  },
  buttonText: {
    color: '#ffffff',
    fontWeight: '600',
    fontSize: 16,
  },
});


