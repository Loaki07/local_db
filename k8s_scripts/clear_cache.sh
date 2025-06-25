#!/bin/bash

# example
# CONTEXT="o2"                           
# NAMESPACE="openobserve"                   
# STATEFULSET_NAME="o2-openobserve-querier" 
# TARGET_DIR="/data/cache/results"          
# CONTAINER_NAME=""  

# ---------------- CONFIGURATION ----------------
CONTEXT=""                           # Set your kube context here
NAMESPACE=""                   # Namespace where StatefulSet is running
STATEFULSET_NAME="o2-openobserve-querier" # StatefulSet name
TARGET_DIR="/data/cache/results"          # Directory to delete inside each pod
CONTAINER_NAME=""                         # Optional: set if multiple containers per pod
# ------------------------------------------------

# Use kubectl with context and namespace
KUBECTL="kubectl --context=$CONTEXT -n $NAMESPACE"

# --------- CHECKS ---------

# Check if context exists
if ! kubectl config get-contexts "$CONTEXT" &>/dev/null; then
  echo "❌ Context '$CONTEXT' not found in kubeconfig."
  exit 1
fi

# Check if namespace exists
if ! $KUBECTL get namespace "$NAMESPACE" &>/dev/null; then
  echo "❌ Namespace '$NAMESPACE' does not exist in context '$CONTEXT'."
  exit 1
fi

# Check if StatefulSet exists
if ! $KUBECTL get statefulset "$STATEFULSET_NAME" &>/dev/null; then
  echo "❌ StatefulSet '$STATEFULSET_NAME' not found in namespace '$NAMESPACE'."
  exit 1
fi

# Try to get pods using label selector
PODS=$($KUBECTL get pods -l app.kubernetes.io/name=$STATEFULSET_NAME -o jsonpath='{.items[*].metadata.name}')

# Fallback: grep pod names directly
if [ -z "$PODS" ]; then
  PODS=$($KUBECTL get pods -o name | grep "$STATEFULSET_NAME" | awk -F/ '{print $2}')
fi

if [ -z "$PODS" ]; then
  echo "❌ No pods found for StatefulSet '$STATEFULSET_NAME'."
  exit 1
fi

# Count the number of pods
POD_COUNT=$(echo "$PODS" | wc -w)

# echo the pods
echo "Number of pods: $POD_COUNT"
echo "✅ Found pods:"
echo "$PODS"

# --------- DELETE DIRECTORY IN EACH POD ---------

for POD in $PODS; do
  echo "🔍 Checking pod $POD..."

  # Check if directory exists before deleting
  if [ -z "$CONTAINER_NAME" ]; then
    DIR_EXISTS=$($KUBECTL exec "$POD" -- test -d "$TARGET_DIR" && echo "yes" || echo "no")
  else
    DIR_EXISTS=$($KUBECTL exec -c "$CONTAINER_NAME" "$POD" -- test -d "$TARGET_DIR" && echo "yes" || echo "no")
  fi

  if [ "$DIR_EXISTS" = "yes" ]; then
    echo "🗑️ Deleting directory $TARGET_DIR in pod $POD..."
    if [ -z "$CONTAINER_NAME" ]; then
      $KUBECTL exec "$POD" -- rm -rf "$TARGET_DIR"
    else
      $KUBECTL exec -c "$CONTAINER_NAME" "$POD" -- rm -rf "$TARGET_DIR"
    fi
  else
    echo "⚠️ Directory $TARGET_DIR does not exist in pod $POD. Skipping."
  fi
done

echo "✅ All done!"
