// Portions of shader code has been taken from Three.js source code.
//
// Copyright © 2010-2021 three.js authors

#version 300 es

precision mediump float;

uniform mat4 projection;
uniform mat4 modelView;

in vec3 position;
in vec3 normal;

#ifdef USE_INSTANCING
    in mat3 instancedNormalMatrix;
    in mat4 instancedModelView;
    #ifdef USE_INSTANCED_COLORS
        in vec4 instancedColor;
    #else
        uniform vec4 color;
    #endif
#else
    uniform vec4 color;
    uniform mat3 normalMatrix;
#endif

out vec3 vViewPosition;
out vec3 vNormal;
out vec4 vColor;

void main() {
    #ifdef USE_INSTANCING
        vNormal = normalize(instancedNormalMatrix * normal);
        #ifdef USE_INSTANCED_COLORS
            vColor = instancedColor;
        #else
            vColor = color;
        #endif
    #else
        vColor = color;
        vNormal = normalize(normalMatrix * normal);
    #endif
    vNormal.y = -vNormal.y;
  
    vec4 adjustedPosition = vec4(position, 1.0);
    mat4 modelViewTransformed = projection * modelView;
    #ifdef USE_INSTANCING
        modelViewTransformed *= instancedModelView;
    #endif

    gl_Position = modelViewTransformed * adjustedPosition;
    vViewPosition = -adjustedPosition.xyz;
}
