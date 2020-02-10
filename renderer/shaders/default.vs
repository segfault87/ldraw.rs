#version 140

precision mediump float;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;
uniform mat3 normalMatrix;

attribute vec3 position;
attribute vec3 normal;

varying vec3 vViewPosition;
varying vec3 vNormal;
varying mat4 vInvertedView;

void main() {
  vNormal = normalize(normalMatrix * normal);
  vNormal.y = -vNormal.y;
  vInvertedView = inverse(viewMatrix);
  
  vec4 adjustedPosition = vec4(position, 1.0);
  vec4 mvPosition = modelView * adjustedPosition;
  
  gl_Position = projection * viewMatrix * mvPosition;
  vViewPosition = -mvPosition.xyz;
}
